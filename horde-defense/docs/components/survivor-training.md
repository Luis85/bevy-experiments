---
type: View
id: survivor-training
title: "Survivor training"
parent: survivor
status: Implemented
source: "crates/horde_game/src/ui/character.rs::survivor"
---

# Survivor training

Allocate supported training and acquire a training point when legal.

- **Entry, exit and actions:** Use Buy 1 training CP, choose a supported Train action, or redistribute advancement; return to equipment or briefing.
- **State boundary:** Campaign rules validate costs, awarded-point history, allowance and supported attribute caps. Policy does not alter roll outcomes.
- **Responsiveness and accessibility:** Costs and disabled reasons are written. Earned objectives distinguish ranged hits/combat time from paid training.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Fresh advanced controls require the learned feature node and applicable liberation milestone; legacy capabilities remain. Mara/recruit advancement retains its individual rules budget and is not bought through personal body nodes.
