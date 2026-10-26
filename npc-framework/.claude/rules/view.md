---
paths:
  - "content/village_view/**"
  - "crates/npc_view_helpers/**"
  - "apps/village/**"
  - "apps/rts_units_demo/**"
  - "content/testbed_view/**"
  - "apps/actor_testbed/**"
  - "content/**/view/**"
  - "content/**/*view*.rs"
---

# Presentation code: read the sim, never write it

The view exists so a human can watch the sim. The sim must give the same state hash with or without it.
Generic helpers live in `crates/npc_view_helpers` (no content); the village's models, animation, camera,
scene and overlay in `content/village_view`; key bindings for the sim speed in `apps/village`.

## Rules

- **Read-only on sim state.** Never mutate sim components (`SimPos`, `Needs`, `Stockpile`, `BrainState`,
  `ActiveLeaf`, `Reservations`, action components) or sim resources. Change the sim only by submitting a
  `SimCommand` (`npc_framework::prelude::submit`, or `npc/command` over BRP), which is recorded for replays.
- **Separate visual entities.** Each sim entity gets its own visual entity linked by `VisualOf(Entity)`.
  Never insert `Transform`, meshes, materials, animation players or picking components on sim entities.
- **Interpolate, don't simulate.** Smooth motion lerps `PrevSimPos → SimPos` with
  `npc_view_helpers::interp::interp_alpha` (the kernel driver's own `SimTime`, never `Time<Fixed>`); floats
  are fine here, but they must never flow back into the sim.
- **Animation from state.** Pick clips from `Activity` / the active action component; no gameplay logic in
  animation or camera code.
- **Selection, overlays, panels are view-only.** Selection is a view resource. The journal (F3) and perf
  (F4) panels read from `Journal` and the profiler; keep their refresh cheap (no full `report()` per frame).
- **Keys belong to the app.** Bindings (Space, 1–4, `.`, F1/F3/F4, camera) live in `apps/*` or view code,
  never in a library/sim crate. A library plugin must not write `Time<Fixed>` or other host resources.
- **The driver** is the kernel's (`NpcPlugins::run_in(..)`); the view never runs `NpcSimTick` itself.
  Speed and pause go through `SimControl` (bound to keys in the app); they never change what one tick does.
- Bevy 0.20 APIs: check `docs/research/bevy-0.20-cheatsheet.md` or the registry source before use.

## Proving it

- Headless tests cannot see the screen: use `/verify-visual` (screenshot at a fixed tick, Read the PNG).
- `cargo xtask verify` (view changes must not change any replay hash or golden).
