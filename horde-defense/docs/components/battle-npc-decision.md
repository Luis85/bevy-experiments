---
type: View
id: battle-npc-decision
title: "NPC decision and policy popup"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/field.rs::battle; crates/horde_game/src/inspector.rs::npc_decision"
---

# NPC decision and policy popup

Explain the active NPC's current action and permit policy edits.

- **Entry, exit and actions:** In NPC mode open Tactics; the popup is titled "<name> · tactics" for whoever holds the defense (Mara, or a recruit taking over, whose tactics it edits through `SetRecruitTactics`); inspect its decision (headed "<name> · current decision") and adjust its tactics; dismiss outside the popup.
- **State boundary:** Decision context is read-only; policy changes use the existing validated command at the next legal decision.
- **Responsiveness and accessibility:** The popup wraps and scrolls within field height; NPC inspection never masquerades as personal firing input.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


NPC zero/negative injury HP can mean conscious Critical / still fighting under the explicitly retained GURPS rules. Signed HP and CRIT · FIGHTING labels distinguish that state from an actual terminal. Actual defeated actors stop and reap/pay in the resolving fixed tick. Hover inspection hit-tests the drawn body (Enemy.y() plus the presentation-only crowd_stagger, W1-69), so each body of a crowd can be inspected; targeting and hits still resolve on the simulation position. Enemies never pass or queue behind each other; a saved Queued phase reads as Approaching.
