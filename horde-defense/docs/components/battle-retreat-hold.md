---
type: View
id: battle-retreat-hold
title: "Hold to retreat"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/retreat.rs::retreat_button, hold_step, retreat_fill, RETREAT_HOLD_SECS"
---

# Hold to retreat

End a defense deliberately, without a modal over the field.

- **Entry, exit and actions:** "Retreat & train" in the personal bar ("Recall & train" for Mara or a recruit). Hold it with the primary mouse button, or focus it and hold Enter or Space: an amber fill grows left to right and the label reads "Hold to retreat…" ("Hold to recall…"). After 0.8 s it dispatches `Command::Withdraw` and the round settles on Results. Releasing early cancels and shows "Hold to retreat" for 1.5 s; Escape cancels a hold. Hover: "Hold 0.8 s to end this defense and open Results. Earned coins and items stay banked."
- **State boundary:** The hold is `BattleUi.retreat_hold`, counted in wall time; a rerun egui pass adds no time and a hold left by a battle that ended under it does not carry over. Core's withdrawal owns the outcome.
- **Responsiveness and accessibility:** The button is sized for the wider of its two labels, so the bar does not move when the hold starts. Reduced effects shows a stepped fill (0, 25, 50, 75, 100%) instead of a smooth one.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
