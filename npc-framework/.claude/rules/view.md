---
paths:
  - "content/village_view/**"
  - "crates/npc_view_helpers/**"
  - "apps/village/**"
  - "apps/rts_units_demo/**"
  - "content/testbed_view/**"
  - "apps/actor_testbed/**"
  - "content/island_view/**"
  - "apps/island_game/**"
  - "content/**/view/**"
  - "content/**/*view*.rs"
---

# Presentation code: read the sim, never write it

The view exists so a human can watch the sim. The sim must give the same state hash with or without it.
Generic helpers live in `crates/npc_view_helpers` (no content). The shipped game's view is
`content/island_view` (terrain, residents, buildings, camera, player HUD `player_hud`, colony panels,
shell, audio); its host `apps/island_game` owns the screens, pause, save lifecycle and key bindings
(`frontend`, `island::session`). The examples' views are `content/village_view` (host `apps/village`) and
`content/testbed_view` (host `apps/actor_testbed`). `island_view` never depends on a `testbed_*` or
`village_*` crate, nor they on it (ADR 0044): the island's copies of shared view code have diverged.

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
- **Island menus gate the view, not the sim.** `FrontendGate` suppresses gameplay input and automatic view
  work while `island_game`'s menus are open; it never becomes simulation state. `player_hud` presents and
  submits shared, recorded actions; hidden workspaces build no observations; profile through the existing
  view tags and never change authoritative scheduling by UI visibility (`docs/island-ui.md`, ADR 0036).
- **Assets the island loads are listed.** A model, texture or sound `island_view` loads must be listed in
  `assets/living-world-manifest.json` (CC0, with source, bytes and sha256); catalog props and sounds are
  only used when listed (`island_world/props.rs`, `island_audio/catalog.rs`).
- Bevy 0.20 APIs: check `docs/research/bevy-0.20-cheatsheet.md` or the registry source before use.

## Proving it

- Headless tests cannot see the screen: use `/verify-visual` (screenshot at a fixed tick, Read the PNG;
  the island: `cargo run --locked -p island_game -- --scenario living_world --screenshot PATH --after-secs S`).
- `cargo xtask verify` (view changes must not change any replay hash or golden).
