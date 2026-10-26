### Spec Compliance

- ✅ Spec compliant. All listed files have hunks; interfaces and rules match the brief.
- ⚠️ Cannot verify from the diff: the real mouse path (PointerClick observer -> ClickedUnit -> mouse_input) and the `world_to_viewport` fill are untested (as the report says). Put them on the demo checklist: click a unit, box-drag under camera rotation/zoom, double-click, Shift variants, Alt+digit.
- ⚠️ docs/debugging.md method table (line ~359) is not updated for fl.select/fl.selection. Plan line ~1518 suggests a later docs task, so confirm that task covers it.

### Strengths
- Selection and ControlGroups hold SimIds, never Entities. prune_selection (selection.rs:387) drops units absent from `SnapshotBuffer.curr` in both. A LocalPlayer switch clears the selection.
- The selection state is client-only and never touches sim state.
- Ownership is enforced at every entry: click, box, group recall, `fl.select`.
- Double-click compares the snapshot record's `unit` type id (selection.rs:605). It also requires the same unit id and a 350 ms window, and an enemy or ground click resets `last_click`.
- Gesture logic is pure and testable: `apply_input`, `box_select` and `classify_release` are separate. `OnScreen` is filled lazily from `Camera::world_to_viewport`, which uses the active camera's GlobalTransform, so rotation and zoom are covered. Units behind the camera return Err and are skipped.
- Gating on console, egui and spawn-panel placing is done in one SystemParam.
- API check (bevy_picking-0.20.0 events.rs:350-359): `PointerClick` is a real EntityEvent with `entity`, `button` and auto_propagate. `On::propagate(bool)` exists (bevy_ecs system_param.rs:139). `trigger.rs:317` re-targets `entity` to the current observed entity during propagation, so `views.get(click.entity)` on the visual root works for child-mesh hits.
- MeshPickingPlugin is added only if absent. fl_game already adds it (builder.rs:295).
- Method names fl.select / fl.selection are consistent with the existing fl.* naming.
- 19 selection tests plus a BRP test.

### Issues

#### Critical
- None.

#### Important
- None.

#### Minor
- remote.rs:~52-76: `selected_json`, `select` and `selection` were inserted between the `/// The newest snapshot received...` doc comment and `fn latest_snapshot`. The doc now attaches to `selected_json`, and `latest_snapshot` loses its doc. Move the new functions above that comment.
- remote.rs:~25: the `fl.select` description string is missing a trailing `\`, so the literal text contains a run of spaces ("ids          are ignored") in the BRP discover output. Fix the continuation.
- selection.rs:531: `fill_on_screen` takes the first active Camera. With any second active camera (UI or minimap) the projection could be wrong. Filter on `With<RtsCamera>`.
- selection.rs:528-531: when there is no camera or no snapshot, `OnScreen` is silently left stale. The double-click and box paths would then use old positions. Clear it instead.
- selection.rs:630-634: recalling an empty or fully dead group is a no-op and keeps the current selection. This is defensible and the report documents it, but it is untested. The tests cover the dead-unit prune but not Recall on a dead group.
- tests/selection.rs: untested paths are the egui and pointer gating, the `PressState` tiny-drag-to-click flow, and a Shift double-click. `draw_rings` is only covered through `ring_radius`.
- selection.rs:56 (`select_own`): it reads `SnapshotBuffer.curr`, while the neighbouring `fl.hash` path uses `latest_snapshot` (which counts the backlog). Right after `fl.step`, `fl.select` could judge ownership against a stale snapshot.

### Assessment

**Task quality:** Approved

**Reasoning:** The logic is correct, client-only and SimId-based, and the Bevy 0.20 `PointerClick` usage is real. The remaining findings are polish: a misattached doc comment, a string typo, and a few small robustness points. The untested mouse and viewport wiring is small, and the report lists it for the demo checklist.
