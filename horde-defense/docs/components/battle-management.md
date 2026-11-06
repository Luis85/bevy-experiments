---
type: View
id: battle-management
title: "Battle management"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/field.rs::battle; crates/horde_game/src/ui/battle/details.rs::field_details"
---

# Battle management

Expose settings, saving, retreat, encounter context and resolution history on demand.

- **Entry, exit and actions:** Open Management; save deployment, save and return to menu, open Settings or read the combat log; close to return to the field. Withdrawal is the single visible "Retreat & train" (or "Recall & train") action beside it.
- **State boundary:** Save and withdrawal are runtime intents. inspect_log is presentation state; reading details does not resolve additional actions.
- **Responsiveness and accessibility:** Text wraps within a bounded menu; optional log avoids crowding combat while preserving traceable damage/decision context.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Fresh advanced controls require the learned feature node and applicable liberation milestone; legacy capabilities remain. Mara/recruit advancement retains its individual rules budget and is not bought through personal body nodes.
