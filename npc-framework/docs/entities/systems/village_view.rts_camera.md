---
type: System
---
# `rts_camera` (village view)

**Crate:** `village_view` · **File:** [`content/village_view/src/camera.rs`](../../../content/village_view/src/camera.rs) · **Plugin:** `VillageViewPlugin` (`camera::plugin`)

The village's orbit camera. WASD / arrow keys pan the focus on the ground plane (screen-relative, faster when
zoomed out, clamped to ±20 m), Q/E orbit the yaw, the mouse wheel zooms (distance × 0.9 per line, clamped to
6–60 m). The camera's `Transform` is rewritten from its `RtsCamera { focus, yaw, pitch, distance }` only
when it changes. `spawn_camera` (in `Startup`) spawns the `Camera3d` with `RtsCamera::default()` (focus at
the origin, 45° pitch, 25 m).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Input`
- **Ordering:** none beyond the set.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`, `ButtonInput<KeyCode>` (optional), `AccumulatedMouseScroll` (optional) |
| writes | `RtsCamera`, camera `Transform` (view entity) |
| messages / events | none |

## Determinism

View-only: reads no sim state at all and writes none.

## Performance

- **Mechanic tag:** `view.camera` (prefix `village_view::camera`) · **Budget:** 100 µs (`view.camera`) ·
  no span.

## Related

- [`testbed_view.rts_camera`](testbed_view.rts_camera.md) — the testbed's camera (adds tilt and focus on
  the selected unit).
- Key bindings for sim speed are not here: they live in `apps/village`.
