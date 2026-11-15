---
type: View
id: battle-layout
title: "Battle screen layout"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/layout.rs::BattleLayout, BarRows, TOP_BAND, HINT_BAND; crates/horde_game/src/art/camera.rs::fit_field, clearing_size"
---

# Battle screen layout

One fixed set of rects for the main battle, built once per battle frame from the canvas.

- **Regions:** The top band (124 points) holds the defender HUD (top left), the battle HUD (top right), the attack selector and the receipt slot (loot receipt or the battle toast) between them. The arena is the canvas inset 24 points at the sides, under the top band and 84 points above the bottom; the control bar and the key legend sit under it. `BattleLayout::new(canvas, lanes, width_yards, hint_band)` is pure Rect math over the canvas, the attempt's frozen lane count and width and whether it reserves a first-defense hint band; it reads no settings, simulation or random state.
- **Camera fit:** The field is `art::fit_field` of the geometry's reference clearing inside the arena: one uniform zoom that keeps the lane:yard proportion and touches the arena on its limiting axis, centred. Fewer lanes mean more zoom; every field of 11 lanes or more (17x30 included) is the pre-pass rect bit for bit. A first personal defense keeps a 92-point band above the field for the hint callout, but only while the field still zooms under it. The selector and receipt slot are anchored to the arena, never to the field, so they do not move with zoom.
- **Overlays:** The construction panel overlays the field's right side (at most 280 points wide) and never changes the field; field hover is ignored inside it, and an INCOMING chevron marks spawns it covers. The READY / PAUSED banner sits in a bottom letterbox that holds it, or as a compact tag under the battle HUD, and never on a lane. The boss banner is centred on the arena under the hint callout or the arena top, and ends left of an open construction panel.
- **Two-row bar:** The bar is measured every frame (`BarRows`): personal [Pause | Begin defense | Resume defense] [Emergency aid] [Brace the line] (each once unlocked) [Build defenses | Close construction] (with a blueprint) [Retreat & train] [Management]; NPC [Pause | Begin defense | Resume defense] [Build defenses] (with a blueprint) [Observe 1× 2× 4×] [Tactics] [Recall & train] [Management]. Begin and Resume are amber only while the battle is paused; a running battle has no primary. Reload, Strike contact lane and Toggle firing live in Management › Field actions (their keys and the legend are unchanged), so the bar stays within two rows at 1050x700 and 135% scale. The key legend under the bar follows the mode: a personal defense names fire, melee, reload, build, pause and cycle attack; an observed (NPC) defense names only build and pause, the keys that act while observing (CF-3-31).

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
