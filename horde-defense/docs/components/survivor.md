---
type: View
id: survivor
title: "Mara survivor"
parent: preparation
status: Implemented
source: "crates/horde_game/src/ui/character.rs::survivor"
---

# Mara survivor

Prepare the autonomous defender with training, equipment and a visible tactical policy.

- **Entry, exit and actions:** Choose Mara, follow Meet Mara after the introductory attempt, or enter from readiness; open training/policy or change Mara's equipment.
- **State boundary:** Mara has an independent character record and advancement allowance. The availability message follows campaign progression.
- **Responsiveness and accessibility:** Summary text exposes CP, relevant combat values and the current advancement allowance; detailed rules remain optional.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Fresh advanced controls require the learned feature node and applicable liberation milestone; legacy capabilities remain. Mara/recruit advancement retains its individual rules budget and is not bought through personal body nodes.
