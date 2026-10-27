# Task 14: actor editor evidence

Date: 2026-10-27. Continuation of the whole-plan implementation in the current monorepo checkout.

## Implementation and decisions

- Main → Actors launches `actor_testbed --actor-editor --actors-dir DIR`. Settings exposes the optional actors
  folder; blank uses the default beside the agents folder. Editor CLI supports `--editor-preset ID` and ordinary
  screenshot flags for reproducible captures, rejecting conflicting simulation/explorer flags.
- Three columns: valid/invalid preset list with New/Duplicate; the actual 3D models with model picker and
  Body/Sensor ranges/Rotate controls; structured Identity/Sheet/Actions/Sensors forms. Every preset field,
  skill/trait row and sensor parameter is editable, with live reasons and a checked point total.
- No simulation driver runs in the editor. A frozen headless world supplies validation registries; preview
  entities have no SimIds. Sensor geometry uses real metre ranges; clipping and zoom respect the centre viewport.
- Stable filename IDs survive ordinary edits (notably Human scout remains `human.json`). New/duplicate/renamed
  drafts use the display-name slug and preserve the source. The destination label updates while typing.
- Saves are atomic and protect racing/changed files with version-specific confirmation. Dirty selection/New/Close
  operations require discard approval. Revert restores disk state or an unsaved draft's pristine baseline.
- `sheet_point_total` validates bounds/names before pricing, checks intermediate integer bounds, and widens banked
  points to i64. Extreme hand-edited values cannot overflow the authoritative point-cost code. See ADR 0015.

## Focused checks

- `cargo test --locked --workspace actor_editor::tests -- --nocapture`: **14 passed**, including real Bevy CPU UI
  layout at 1x and 2x scale, filename bindings without recreating the focused field, dirty close/cancel,
  New/Duplicate/Revert, collision races, changed-on-disk versions, live validation, all sensor fields and preview.
- `cargo test --locked -p testbed_view -p game_menu model_preview`: **3 passed** (model picker, real sensor
  geometry/no SimIds, physical viewport bounds). Its fixture initializes the asset required by Bevy's own
  skinned-mesh gizmo system; no production assertions or systems were disabled.
- Native test binaries built by Cargo: editor-mode parser **2 passed**, menu actor-folder/launch tests
  **4 passed**, testbed-content actor tests **14 passed**, including original hash pins and extreme cost cases.
- Full Phase E `cargo xtask verify`: **`"ok": true`** in `target/verify-phase-e-serial.json` with process-local
  `CARGO_BUILD_JOBS=8`, `NEXTEST_TEST_THREADS=1`: 1,105 tests passed (12 skipped; one slow, three reported leaky),
  one doc test passed (ten ignored), architecture 7/7, eight replays, six snapshots with none pending, Clippy
  no warnings and cargo-deny passed. No optional tools were skipped. The earlier concurrent run failed two
  detached-host tests; both passed unchanged in isolation and in the complete serial run. No assertions,
  baselines, protected tooling or lint policy were changed to get the verdict. Five-app release build pending.

## Bounded visual check

Used the repository's verify-visual procedure. First batched Human/Rover captures exposed collapsed single-line
field content and description scroll starting at the end. One correction batch set explicit text-line sizing,
initial scroll/cursor at the beginning and a live destination label, backed by layout/binding tests.

The confirmation captures were opened and inspected:

- `target/shots/task14-editor-human-final.png`: Human scout, 64/100 points, name/speed70/carry30000 values,
  first description lines, model picker, readable structured form, complete character model on its turntable.
- `target/shots/task14-editor-rover-final.png`: Rover, 52/100 points, name/speed60/carry40000, robot body and
  physical sensor domes, correct selected preset/model/body states, actual sensor contours in the preview.

Both apps exited successfully through screenshot capture. Fixtures are under ignored `target/task14-playtest/`;
no real user preset folder/settings were used for captures. All remaining tasks stay behind their phase gates.
