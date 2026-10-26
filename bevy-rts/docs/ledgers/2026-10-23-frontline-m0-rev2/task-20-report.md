# Task 20 report: client view (ClientFeature, LocalDriver, runner link, interpolation, placeholders)

Worktree `C:\fl\t20`, branch `m0/t20-clientview` (from 01c7699). Commit `2d0ae38`
`feat(fl_client): ClientFeature, LocalDriver, runner link, snapshot interpolation, placeholder visuals`
(trailers `Refs: ADR-0002, ADR-0001`, `Co-Authored-By`).

## What I implemented

- `crates/fl_client/src/feature.rs`: `ClientFeature { sim_feature(), build(app) }`.
- `crates/fl_client/src/driver.rs`: `LocalDriver` (Resource) wrapping `fl_view::CommandStamper`.
  - `TICK_LENGTH` = 1 s / `SIM_TICK_HZ` (50 ms); input delay 1 (`exec_tick = next + 1`).
  - `advance(dt)` adds `dt × speed` unless paused. `poll()` emits while the accumulator is ≥ 50 ms. While paused it emits only allowed steps, carrying `take_due(tick)`.
  - `step(n)` counts only while paused; `resume()` drops any unused steps. `set_speed` clamps to [0.25, 8], and NaN becomes 1.0.
  - `overstep_fraction()` is clamped into [0, 1). `restore()` resets next, pending, seq, the accumulator and the steps.
  - `TickDriver` impl.
- `crates/fl_client/src/link.rs`:
  - `RunnerLink`, `SnapshotBuffer` (+ `push` rotate), `SimEventMsg`, `RunnerStatus`.
  - `PendingReplies`, with `take` / `insert` and a private id allocator.
  - `encode_command`: the id from the `"commands"` table by `P::NAME`, postcard payload, errors as text. `send_query`.
- `crates/fl_client/src/view.rs`: `ClientContent`, `LocalPlayer`, `RevealAll`, `UnitView` (also `PartialEq, Eq`), `ViewPose` (+ `at_rest`, `sample(t)`), `ViewIndex`, `sim_to_world`, `heading_to_yaw`, `lerp_yaw` (shortest arc), and a public `SimViewSystems` set.
  - `SimViewPlugin`, in Update and chained:
    - `drive_runner` (runs if `RunnerLink` and `LocalDriver` exist): advances by `Time<Real>` delta, sends every polled `TickInput`, and drains **all** `FromRunner` messages.
      - Tick: rotates the buffer, writes one `SimEventMsg` per event, and sets `last_tick` / `last_hash`.
      - Reply: goes to `PendingReplies`.
      - Failed, or a disconnected channel: sets `RunnerStatus.failed` (the first failure wins, logged with `tracing::error!`) and pauses the driver. Nothing is advanced or sent after a failure.
    - `sync_views`: runs when the snapshot tick changes or `RevealAll` changes.
      - Despawns views (`try_despawn`, recursive) whose record vanished or is no longer drawn.
      - Spawns `(Name, UnitView, ViewPose, Transform, Visibility)` for new records.
      - Shifts poses: `prev` comes from `SnapshotBuffer.prev`'s record when present, so several ticks in one frame still interpolate T-1 → T. Otherwise `prev` is the old `curr`.
    - `interpolate_views`: sets translation and rotation from `ViewPose::sample(overstep_fraction)`.
- `crates/fl_client/src/placeholder.rs`: `PlaceholderPlugin`, `PlaceholderAssets`, `PlayerColors` (+ `from_factions(content, faction_ids)`, palette fallback), and `placeholder_parts(&Placeholder, scale) -> Vec<(Mesh, Transform)>`.
  - Box, Tank (hull + turret + barrel cylinder along +X), Infantry (capsule) and Truck (bed + cab), all with length along local +X.
  - On `Changed<UnitView>` it rebuilds the child meshes. Mesh handles are shared per `UnitTypeId`, built lazily from `VisualDef` with a 2.5×2×5 m box as fallback.
  - One material per `PlayerId` (or a shared neutral material when `recolor` is false).
  - It runs only when `Assets<Mesh>` and `Assets<StandardMaterial>` exist, so it stays headless-safe.
- `crates/fl_client/src/lib.rs`: the new modules, and re-exports `LocalDriver`, `ClientFeature`, `RunnerLink`.
- `crates/fl_client/tests/view.rs`: a `FakeRunner` thread on `fl_view::runner_channel` that answers `ToRunner::Tick` with canned `TickOutput`s, records inputs, and acks each reply. Each test update waits for the acks of the ticks it sent, so the tests are deterministic. No `fl_sim` anywhere.

## Tests

All 13 brief tests: `sim_to_world_axes`, `heading_90_faces_plus_z`, `lerp_yaw_wraps`, `interpolation_midpoint`, `local_driver_stamps_delay_one`, `local_driver_accumulates_fixed_steps`, `speed_clamped`, `paused_driver_emits_nothing_until_step`, `paused_orders_apply_on_resume` (app level: the fake runner receives tick 10 empty and tick 11 carrying the command), `visuals_follow_snapshots`, `despawned_record_removes_visual`, `multiple_outputs_per_frame_keep_all_events`, `first_frame_without_snapshot_draws_nothing`, `runner_failure_is_surfaced`, `encode_command_uses_string_table`.

Extras:
- `local_driver_restore_continues_seq`
- `runner_disconnect_is_surfaced`
- `reveal_all_draws_unseen_records`
- `send_query_routes_reply`
- `tank_placeholder_barrel_points_plus_x`
- `placeholders_share_handles` (MinimalPlugins + AssetPlugin; the same mesh for all, the same material per owner, a different material for another owner)

### TDD evidence

RED: `cargo nextest run --profile agent -p fl_client` (tests written first):
```
error[E0432]: unresolved import `fl_client::driver`
error[E0432]: unresolved import `fl_client::link`
error[E0432]: unresolved imports `fl_client::view::RevealAll`, `fl_client::view::UnitView`, `fl_client::view::ViewIndex`, `fl_client::view::ViewPose`, `fl_client::view::heading_to_yaw`, `fl_client::view::lerp_yaw`, `fl_client::view::sim_to_world`
error: could not compile `fl_client` (test "view") due to 3 previous errors
```
This was expected, because none of the Task 20 API existed yet (the files were stubs).

GREEN: the same command:
```
Starting 33 tests across 2 binaries
Summary [  10.100s] 33 tests run: 33 passed, 0 skipped
```
(21 in `tests/view.rs` + 12 existing camera unit tests). Each test process takes about 5 s on this machine, including the pure ones. That is Windows process start, not test logic.

### Gate (`cargo xtask gate task --track client`, `.gate-base` = 01c7699)
```
PASS fmt (3.7s)
PASS clippy-drift (2.7s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
PASS test-guard (0.7s)
PASS clippy (33.5s)
PASS arch-check (7.7s)
WARN test-headless (65.9s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (23.2s)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 139s
```
`arch-check` passes, so `fl_client` has no `fl_sim` edge. `Cargo.toml` and `Cargo.lock` are untouched. All files are LF.

## Files changed
- Created: `crates/fl_client/src/{feature,driver,link}.rs`, `crates/fl_client/tests/view.rs`
- Modified: `crates/fl_client/src/{view,placeholder,lib}.rs`

## Self-review notes and concerns
1. **Visibility filter.** The client has no player → team map: the header carries none, and `LocalPlayer` is only a `PlayerId`. So a record is drawn when `RevealAll` is on or `visible_to != 0`. In M0 the sim publishes `visible_to = 0xFF`, so this is correct for now. M3 needs a local-team source to narrow it.
2. **Player colours.** The placeholder material needs player → faction, which no Task 20 input provides. I added `PlayerColors` (+ `from_factions`) with a palette fallback. Task 22 should insert `PlayerColors::from_factions(&content, scenario.players.iter().map(|p| p.faction.as_str()))`, or units get palette colours.
3. **No `VisualDef`.** The fallback is a fixed 2.5×2×5 m box, not rev-1's "Box from Hull footprint": `Hull` is a sim def component the client can't name.
4. **Journal entries.** `TickOutput.journal` is dropped by `drive_runner`. Task 22 plans "a PostUpdate system writes every drained `TickOutput.journal`", so it will need a hook here, for example a `SimJournalMsg` message or a buffer resource. This was not in Task 20's interface, so I didn't add one.
5. **Small additions beyond the brief's interface:**
   - `SnapshotBuffer::push`, `PendingReplies::{take, insert}`, `ViewPose::{at_rest, sample}`, the `SimViewSystems` set.
   - `UnitView: PartialEq + Eq`, `TICK_LENGTH` / `MIN_SPEED` / `MAX_SPEED` consts.
   - `PlaceholderAssets::{meshes, material}`, `placeholder_parts`, `PlayerColors`.
6. **No catch-up cap.** `advance` adds the full real `dt × speed` (no 250 ms cap, per the brief), so a long hitch sends a burst of ticks.
7. Bevy facts used, all checked in the 0.20.0 sources:
   - `TimeUpdateStrategy::ManualDuration` advances `Time<Real>`; its first update has a zero delta.
   - `EntityCommands::{try_despawn, despawn_children, with_children}`.
   - `Cylinder::new(radius, height)`, `Capsule3d::new(radius, length)`.
   - The run-condition combinator is `and_then` (I used two `run_if`s).
   - `MessageWriter::write_batch`.
   - None of these were new enough to need a `bevy020-api-reference.md` entry. That reference is outside the client track anyway.

---

## Fix round 1 (commit `fd50e16` `fix(fl_client): receive-timeline interpolation, journal buffer, catch-up clamp`)

### What changed
1. **Receive-timeline interpolation (R42).**
   - New `link::SnapshotAge(Duration)`. It holds the real time since `SnapshotBuffer.curr` arrived: `drive_runner` adds the clamped frame dt and resets it to zero when a tick output is received.
   - New `view::interpolation_fraction(age, tick_period, paused)`. It returns `age / tick_period` clamped to [0, 1], and 1.0 while paused.
   - New `LocalDriver::tick_period()` = `TICK_LENGTH / speed`.
   - `interpolate_views` now uses these instead of `overstep_fraction()`. That method stays public but no longer drives rendering.
   - The `interpolation_midpoint` test now checks `SnapshotAge` = 25 ms instead of the driver overstep. Its x = 5 assertion is unchanged.
2. **Journal forwarded.**
   - New `link::SimJournalMsg(JournalEntry)` message, written once per entry in order, like `SimEventMsg`.
   - New `link::JournalBuffer` resource:
     - A bounded ring (`JOURNAL_CAPACITY` = 10 000, or `with_capacity`) that drops the oldest entry and counts it in `dropped()`.
     - Accessors `push`, `since(tick)` (via `partition_point`), `entries`, `len` and `is_empty`.
   - Both are registered by `SimViewPlugin`. `drive_runner` groups its outputs in a private `Received` SystemParam.
3. **Catch-up clamp.** `LocalDriver::advance` counts at most `MAX_ADVANCE` = 250 ms per call, applied before the speed factor. `SnapshotAge` uses the same clamp.
4. **Placeholder cache invalidation.** `attach_placeholders` rebuilds when `ClientContent` or `PlayerColors` is inserted, changed or removed. It tracks presence in a `Local` and checks `is_changed()`. A rebuild resets `PlaceholderAssets` and re-attaches parts to every `UnitView`, not only changed ones. It carries a scoped `#[expect(clippy::too_many_arguments)]` with a reason.
5. **New tests:**
   - `interpolation_never_moves_backwards`: the fake runner record moves +1 m per tick, frames are 16 ms, and there are 30 updates. x never decreases and moves at least 3 m. After pausing, the view equals the latest record. After `step(1)`, it equals the new latest record (+1 m).
   - `interpolation_fraction_follows_receive_timeline` (pure).
   - `multiple_outputs_per_frame_keep_all_journal_entries`: three outputs with one entry each arrive in one frame. All three are read as `SimJournalMsg` that frame, all three are in `JournalBuffer`, and `since(Tick(1))` returns 2.
   - `journal_buffer_drops_oldest_when_full`.
   - `advance_caps_catch_up`: `advance(10 s)` gives exactly 5 polls.
   - `placeholders_rebuild_when_colors_change`: inserting `PlayerColors` later re-colours an existing view.
   - `despawned_record_removes_placeholder_meshes`: with `PlaceholderPlugin`, the removed record's child mesh entities are gone, and the `Mesh3d` count drops from 3 to 2.

### Evidence that the new interpolation test catches the bug
With `interpolate_views` temporarily switched back to `driver.overstep_fraction()` (then restored):
```
cargo nextest run --profile agent -p fl_client --test view interpolation_never
the view never moves backwards: [0.0, 0.0, 0.0, 0.56, 0.88, 0.19999999, 1.52, 1.8399999, 1.16, 2.48, ...]
Summary [   0.421s] 1 test run: 0 passed, 1 failed, 27 skipped
```

### Tests run
`cargo nextest run --profile agent -p fl_client`:
```
Starting 40 tests across 2 binaries
Summary [  12.511s] 40 tests run: 40 passed, 0 skipped
```

### Gate (`cargo xtask gate task --track client`)
```
PASS fmt (1.2s)
PASS clippy-drift (1.8s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
PASS test-guard (0.6s)
PASS clippy (6.4s)
PASS arch-check (3.8s)
WARN test-headless (27.3s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (20.7s)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 63s
```
The fog filter (`LocalPlayer` is ignored) is deferred to M3, as the review ruled.
