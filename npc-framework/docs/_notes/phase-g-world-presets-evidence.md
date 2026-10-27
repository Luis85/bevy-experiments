# World-preset evidence (2026-10-27)

Tasks 15–17 add strict world presets, optional survival/night/inhabitant mechanics, immutable saved copies,
and the menu's Worlds editor and New/Resume integration. ADR 0017 records the construction and compatibility
decisions. No protected replay, snapshot, golden, lint or dependency-policy files were changed.

## Focused checks

- Content suites passed: library 84 (one ignored), actor presets 14, explorer host 37, robot sensors 6.
- All 17 `worlds` tests passed, including exact Plain compatibility, ongoing movement slowing and stopping,
  direct/shortcut rest and charge recovery, collapsed wrapper rejection, worker/skeleton behavior, snapshot
  equivalence, night scaling without compounding, inhabitants at night, zero-range/speed/threshold boundaries,
  disabled systems including locks/keys/progression, and fresh inhabitant-world recording/replay. Initial
  patrols are validated constructor state; the record contains only actual external inputs.
- All 110 menu tests passed (one ignored). New coverage exercises every world field, live validation, atomic
  publication, changed/colliding files, selected-world actor validation, launch flags, settings and Resume labels.
- Workspace Clippy with all targets and warnings denied passed. The final selected-marker assertion passed;
  all 11 explorer CLI integration tests passed, including copied world rules and taken-port cleanup.

The initial recovery tests called the API before Bevy's plugin finish phase and incorrectly expected action
rejections as RPC errors. Their fixtures now follow production startup and assert exact structured refusals.
The movement test uses the testbed's recorded Move-order/execute-orders path. Positive behavior assertions
remain mandatory; no existing assertions were removed or baselines changed.

## Live Survival run and screenshots

`python target/phase-g-live.py` reports `ok:true` in
`target/phase-g-live-1204af56642f/summary.json`. It starts a real detached human Survival world, takes control
through registration, advances to tick 1,700 and checks rising hunger/thirst/energy, night and 50% movement
speed. It saves/stops, replaces the source Survival file with Plain, resumes native on the same port and
checks that the saved world rules and need levels survive (including exact decay for the elapsed ticks).
The resumed paused observation is tick 1,709. All probe-owned processes exit; logs are sanitized.

Inspected native capture: `target/phase-g-live-1204af56642f/npc-out/shots/survival-night.png`. The human,
recovery sources, walls and reduced visible area render, with explored ground grey and unknown ground black.
Also inspected `target/shots/phase-g-worlds.png` (strict preset list, readable identity fields, pages, save
destination/status) and the initial `phase-g-new.png` (actor/world choices, names, launch modes). The initial
New capture motivated persistent selection markers. The inspected confirmation `phase-g-new-final.png`
clearly marks both selected buttons and retains readable fields and launch controls. No real desktop input
was sent. `phase-g-resume.png` was also inspected: both actor `human` and world preset `survival` appear in
the saved agent's row, with readable launch/token controls.

## Full gate

Full `cargo xtask verify` reports **`"ok":true`** (`target/verify-phase-g.json`): 1,174 tests passed,
12 skipped, two reported leaky; documentation 1 passed/10 ignored; architecture 7/7; all eight replay files
verified; six snapshots none pending; format, Clippy and cargo-deny passed. No optional tools skipped.
Process-local `CARGO_BUILD_JOBS=8`, `NEXTEST_TEST_THREADS=1`; no retries, oracle edits or assertion removals.
Checkpoint `51d804c` was merged --no-ff to main as `7bc8a2c`; the five-app release build on main passed in
3m02s (`target/release-phase-g.log`). Artifacts:
`C:\Projects\bevy-experiments\npc-framework\target\release\{game_menu,actor_testbed,npcctl,village,rts_units_demo}.exe`.
