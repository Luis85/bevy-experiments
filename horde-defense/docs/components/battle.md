---
type: View
id: battle
title: "Active defense battlefield"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/battle.rs::battle"
---

# Active defense battlefield

Present the fixed survivor and deployment-frozen lanes as an atmospheric pixel battlefield.

- **Entry, exit and actions:** Enter on deployment/retry or resume an active defense; aim/fire/reload/melee in personal mode, manage construction or policy on demand, retreat to debrief.
- **State boundary:** Attempt state and fixed ticks own outcomes. Pointer aim, construction visibility and effects are presentation; visuals cannot alter combat geometry.
- **Responsiveness and accessibility:** All deployment-frozen lanes remain visible. Compact corner HUD replaces a permanent sidebar; settings expose outlines, reduced effects and visual cues for important audio.

**Current increment contract:** Deployment/retry start immediately. Failure describes retreat, rest and training with earned rewards retained. Fallen hostile records persist for the attempt and save/resume; settled corpses render in batches without remaining active hostiles or paying twice. Ordinary enemies and skill nodes use authored catalogs.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Focused battlefield hover uses the custom crosshair and exact ammo/reload readout; standard native cursor visibility restores over UI, menus, popups and focus loss. Larger outlined resolved hit numbers use a bounded 0.9-real-second cache, with weak-point/armor/incoming/miss/defense distinctions, a punch/settle/rise and a reduced path without punch/drift. They issue no action and change no authoritative outcomes.

The actual clearing uses frozen lane_count and width_yards, growing from the smaller Normal three-row/twelve-yard opening. Live DPS is the last five simulation seconds of resolved injury divided by five, with a zero-padded opening; hover exposes run average/peak and source totals. Pause/save/4× observation preserve its model clock. Input bounds remain the clearing; bounded feedback may rise into the surrounding effects region.

The displayed defender resolves frozen EquipmentVisual with the current authoritative action sample through player_model. Its submitted pose supplies the muzzle; a bounded historical source-space release cache supplies already-flying throws. The same pixels/sockets feed previews, with no independent gun origin or authoritative movement. Current equipped/action native acceptance remains separate from accepted original bind parity.
