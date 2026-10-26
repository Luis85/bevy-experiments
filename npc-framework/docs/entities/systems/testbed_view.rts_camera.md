---
type: System
---
# `rts_camera` (testbed view)

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/camera.rs`](../../../content/testbed_view/src/camera.rs) · **Plugin:** `TestbedViewPlugin` (`camera::plugin`)

The testbed's orbit camera, like the village's plus tilt and focus. WASD / arrows pan (faster when zoomed
out), Q/E orbit, PageUp/PageDown tilt (pitch 0.35–1.45 rad), the wheel zooms (5–80 m), F centres on the
selected unit's `SimPos`. The focus is clamped to ±30 m. Before it, `focus_at_start` centres on the
preselected unit once it exists (the `CameraFocusOnSelected` request, e.g. `--select`) and then removes the
request. `spawn_camera` (in `Startup`) uses `CameraStart` if inserted, else `RtsCamera::default()` (the
whole compound).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Input` (gated by `not(modal_open)` when the explorer's `ExplorerModalPlugin` is added,
  and by `not(editor_typing)` while a field of the F7 notes editor has focus)
- **Ordering:** `(focus_at_start, rts_camera).chain()`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`, `ButtonInput<KeyCode>`, `AccumulatedMouseScroll`, `Selected`, `SimIndex` (all but `Time` optional); sim `SimPos` |
| writes | `RtsCamera`, camera `Transform` (view entity); removes the `CameraFocusOnSelected` resource |
| messages / events | none |

## Determinism

View-only: reads one sim position, writes none.

## Performance

- **Mechanic tag:** `view.camera` (prefix `testbed_view::camera`) · **Budget:** 100 µs (`view.camera`) · no span.

## Related

- [`village_view.rts_camera`](village_view.rts_camera.md) — the village's camera.
- [`click_select`](click_select.md), [`command_input`](command_input.md) — set `Selected`.
