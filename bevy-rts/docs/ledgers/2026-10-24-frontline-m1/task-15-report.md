# Task 15 report: selection

Status: DONE_WITH_CONCERNS. Commit on m1/t15-selection (see git log).

Implemented crates/fl_client/src/selection.rs (Selection, ControlGroups, ClickedUnit, OnScreen, SelectInput, GroupAction, box_select, classify_release, ring_radius, select_own, SelectionPlugin), added to ClientPlugins; fl.select / fl.selection in debug/remote.rs; tests in tests/selection.rs (19) and brp_select_and_read_selection in tests/remote.rs.

Tests: `cargo nextest run --profile agent -p fl_client --features debug_remote`: 89/89 passed. Gate: `gate task: PASS in 425s` (all steps PASS, api SKIP; WARN over 300s budget, caused by cold builds).
TDD RED was not recorded separately (tests and implementation written together; first compile run had a borrow error, two test-harness bugs fixed in tests).

Deviations / concerns
- Bevy 0.20 has no `Pointer<Click>`; the observer uses `On<PointerClick>` (per docs/research). Observer wiring, mouse/window path and `world_to_viewport` fill are not headless-tested (PointerClick's Pointer has a private field); cover with demo checklist/brp.
- Tests push snapshots directly into SnapshotBuffer rather than using a fake runner thread (deterministic, no threads).
- Choices where the brief was silent: a click on an enemy acts like empty ground (clears unless Shift); recalling/adding/centering an empty or fully dead group does nothing; groups are pruned by snapshot presence, recall filters by ownership; ring uses hull (Tank) / cab+bed length (Truck) for the larger side.
- MeshPickingPlugin is added by SelectionPlugin only if Assets<Mesh> exists and it is absent (fl_game already adds it).
- No drag-box rectangle is drawn (not in the brief).

## Fix round 1
Changed: (1) latest_snapshot doc comment re-attached in remote.rs; (2) fl.select description continuation fixed; (3) fill_on_screen projects through `With<RtsCamera>` cameras only; (4) fill_on_screen clears OnScreen when it runs without a snapshot, player or camera (new pub sets `FillOnScreen`/`ApplySelectInput` let headless tests inject OnScreen after it); (7) `select_own(world, &snapshot, ids)` now takes the snapshot, `fl.select` passes `latest_snapshot(world)` (counts the backlog).
New tests: recalling_an_empty_or_dead_group_does_nothing, shift_double_click_adds_the_type_to_the_selection, on_screen_is_emptied_without_a_camera, tiny_drag_reaches_the_sim_as_a_click_and_a_long_one_as_a_box (real mouse_input path through a PrimaryWindow with a set cursor position).
Tests: `cargo test -p fl_client --test selection`: 23/23 passed. Gate: `gate task: PASS in 420s` (WARN over 300s budget, cold build).
