# Task 13: robots in the explorer — evidence

Date: 2026-10-27. Workspace: `C:\Projects\bevy-experiments\npc-framework`.

The user chose to optimize traversal while preserving complete LIDAR scans. No perception/fog budget,
dependency version, replay baseline, golden, architecture test or lint configuration was changed.

## Implementation

- The view honors `ActorModel` for all six supported models. Rover and Sentinel use procedural bodies,
  wheels, sensor domes and tool grips; Sentinel has a raised sensor mast.
- New-agent choices are loaded from the configured actors folder and fully validated against the explorer's
  registries. Only valid presets are offered; none valid disables Start. Both launch modes pass the selected
  preset; Resume rows display the stored preset. Workspace text describes each robot's actual senses/actions.
- Camera cones, LIDAR beam geometry and radar target cells reveal fog. `VisibleCells.identified` separates
  named static objects from LIDAR-only coverage; units still require sensed percepts. See ADR 0014.
- Range and fog scans share a bitmap supercover walker. Scaled crossing times replace per-cell axis-distance
  updates; cache entries/buffers remain allocated while moving. Camera identity coverage updates on its scan
  phase and is reused by the staggered LIDAR phase, including after snapshot load.

## Checks

- `cargo build --locked -p actor_testbed -p game_menu -p npcctl`: passed.
- `cargo test --locked -p testbed_view -p game_menu -p npc_perception -p testbed_content`: passed during iteration.
- `cargo test --release --locked -p npc_perception -p testbed_content --lib --test fog --test robot_sensors`:
  115 passed, 1 ignored (the existing OS clipboard test). Includes 9,000 randomized beam paths checked against
  `SpatialGrid::trace_segment`, corner side-cell order, overflow-safe hit distance, geometry-only LIDAR,
  camera/radar coverage, moving obstacles and sensor changes, unchanged hashes, and robot fog/readings resume.
- `cargo test --release --locked -p npc_perception -p testbed_content --test explorer_perf -- --ignored --nocapture`:
  passed. The existing profiler uses 300 warm-up ticks, 3,000 measured ticks and five interleaved rounds.
  Moving-rover median p95: perception **37.2 µs**, fog **49.4 µs**, against **55 µs** each. Idle-rover p95:
  perception 7.7 µs, fog 3.8 µs. Moving-Sentinel p95: perception 15.2 µs, fog 13.2 µs. Rounds vary with host load;
  these are measurements, not hard per-frame guarantees. Full scans and sensor cadence are preserved.
- Full `cargo xtask verify`: **`"ok": true`**, saved in `target/verify-task13-final.json` (process-local
  `CARGO_BUILD_JOBS=8`, `NEXTEST_TEST_THREADS=4`). Formatting passed; Clippy reported no warnings; all 1,080
  tests passed (12 skipped, two reported leaky); doc tests: one passed, ten ignored; all seven architecture
  tests and eight replays passed; six snapshots with none pending; cargo-deny advisories, bans, licenses and
  sources passed. No optional tools were skipped. The runner's two leaky-test reports remain a test-harness
  limitation; this change does not claim to fix them. The first run's sole failure was a complex-query-type
  Clippy diagnostic, corrected with a filter type alias and grouped parameters without disabling the lint.

## Local playtest and screenshots

Used `.claude/skills/verify-visual/SKILL.md`. Images were opened and inspected, not inferred from compilation.
All runtime fixtures/settings are under ignored `target/task13-playtest/`; user agent folders/settings were not used.

- `target/npc-out/shots/task13-rover-tick1200.png`: paused at tick 1200, state hash `a894fd4a0c827c34`.
  `npc/unit` #62 reports `(0, -6500)` mm; the orange rover is at that position with wheels and two domes.
  Ground/walls render, unseen ground stays black behind walls, and F6 reports LIDAR 20 m/360 beams plus camera 15 m.
- `target/npc-out/shots/task13-rover-tick1220.png`: an accepted recorded `agent/act` orders a move to
  `(3000, -6500)` mm and executes orders. After 20 stepped ticks, #62 is `(1200, -6500)` mm, state hash
  `c9aec76490865ffb`. The image shows the moved/turned rover, changed camera detections and dimmed last-seen items.
  Full `npc/unit` replies are in `target/task13-playtest/rover-tick{1200,1220}.unit.json`.
- `target/npc-out/shots/task13-rover-final.png`: final model after narrowing tire spacing to an outer width
  of 0.96 m for the world's 1 m door cells. Captured paused at tick 1600, hash `87251c4ee8cdd7fd`, with #62 at
  `(3000, -6500)` mm after completing the move. The unit reply is `target/task13-playtest/rover-final.unit.json`.
- `target/shots/task13-sentinel.png`: blue Sentinel rendered in its radar/camera world, with cone coverage and
  isolated radar target cells rather than a LIDAR circle. F6 reports radar 30 m and camera 15 m.
- `target/shots/task13-menu-new.png`: Human scout, Rover and Sentinel choices, selected actor description,
  name fields and both Start buttons are visible and fit.
- `target/shots/task13-menu-resume.png`: the saved Rover row includes `actor: rover` and its save date.

Both detached fixture hosts stopped through `session/stop`. The rover window saved/exited through `app/quit`;
the Sentinel and menu captures exited through their screenshot flags. No playtest process was left running.

## Remaining plan

Task 13 is complete in the current checkout, with uncommitted changes. Task 14 (actor editor) is next.
Gate E remains unfinished until Task 14; no phase merge or five-app release build was performed for this
Task 13 checkpoint. The three app binaries used for the playtest were built in the dev profile.
