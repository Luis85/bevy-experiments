---
type: View
id: survivor-ledger
title: "Survivor rules and ledger"
parent: survivor
status: Implemented
source: "crates/horde_game/src/ui/character.rs::survivor"
---

# Survivor rules and ledger

Explain supported attributes, historical advancement and combat-resolution stages.

- **Entry, exit and actions:** Expand Character ledger & combat rules; collapse to return to training.
- **State boundary:** Read-only rules context; zero HP is not automatically death and redistribution preserves historical awards.
- **Responsiveness and accessibility:** Attack-roll percentages explicitly precede defense/armor. Explanations distinguish recovery-capable conditions from terminal outcomes.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

