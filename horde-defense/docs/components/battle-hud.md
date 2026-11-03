---
type: View
id: battle-hud
title: "Compact battle HUD and actions"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle.rs::corner_hud, personal_controls"
---

# Compact battle HUD and actions

Show personal identity/portrait, health, ammunition, stamina, boss/time, threat/groups/loot level and essential combat actions.

- **Entry, exit and actions:** Visible during active defense; use Reload, melee, firing toggle or unlocked emergency aid/brace controls as applicable.
- **State boundary:** HUD reads Attempt and its frozen progression/profile snapshot; action controls submit input to the runtime. Displayed values never advance the simulation.
- **Responsiveness and accessibility:** Mouse alternatives and current binding tooltips supplement remappable keys; numerical readouts accompany bars.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


NPC zero/negative injury HP can mean conscious Critical / still fighting under the explicitly retained GURPS rules. Signed HP and CRIT · FIGHTING labels distinguish that state from an actual terminal. Actual defeated actors stop and reap/pay in the resolving fixed tick. Inspection and targeting follow Enemy.y(), including saved passing displacement.

Focused battlefield hover uses the custom crosshair and exact ammo/reload readout; standard native cursor visibility restores over UI, menus, popups and focus loss. Larger outlined resolved hit numbers use a bounded 0.9-real-second cache, with weak-point/armor/incoming/miss/defense distinctions, a punch/settle/rise and a reduced path without punch/drift. They issue no action and change no authoritative outcomes.

The actual clearing uses frozen lane_count and width_yards, growing from the smaller Normal three-row/twelve-yard opening. Live DPS is the last five simulation seconds of resolved injury divided by five, with a zero-padded opening; hover exposes run average/peak and source totals. Pause/save/4× observation preserve its model clock. Input bounds remain the clearing; bounded feedback may rise into the surrounding effects region.
