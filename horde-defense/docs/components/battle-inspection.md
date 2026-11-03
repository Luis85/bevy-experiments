---
type: View
id: battle-inspection
title: "Battlefield object inspection"
parent: battle
status: Implemented
source: "crates/horde_game/src/inspector.rs::hover, dossier"
---

# Battlefield object inspection

Inspect the hostile or defense under the pointer without changing its state.

- **Entry, exit and actions:** Hover an eligible actor/structure in active or observed battle; move away to close.
- **State boundary:** Read-only stable-ID target selection and authored profiles; inspection performs no rolls and mutates no simulation.
- **Responsiveness and accessibility:** The bounded tooltip names lane, HP, protection and state; boss armor/exposure is labeled in text.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


NPC zero/negative injury HP can mean conscious Critical / still fighting under the explicitly retained GURPS rules. Signed HP and CRIT · FIGHTING labels distinguish that state from an actual terminal. Actual defeated actors stop and reap/pay in the resolving fixed tick. Inspection and targeting follow Enemy.y(), including saved passing displacement.

Focused battlefield hover uses the custom crosshair and exact ammo/reload readout; standard native cursor visibility restores over UI, menus, popups and focus loss. Larger outlined resolved hit numbers use a bounded 0.9-real-second cache, with weak-point/armor/incoming/miss/defense distinctions, a punch/settle/rise and a reduced path without punch/drift. They issue no action and change no authoritative outcomes.
