---
type: View
id: survivor-policy
title: "Survivor tactical policy"
parent: survivor
status: Implemented
source: "crates/horde_game/src/ui/character.rs::policy"
---

# Survivor tactical policy

Set legal autonomous action preferences and a recall threshold.

- **Entry, exit and actions:** Choose a policy, aim/reload/risk preferences and recall percentage; changes apply at the next legal decision. Also accessible from the battle Survivor popup.
- **State boundary:** Proposed controls are scene state until SetNpcPolicy is accepted; they cannot reset timers or change resolved dice.
- **Responsiveness and accessibility:** Labels describe risk and 0% recall behavior. The recall slider width adapts to available space.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

