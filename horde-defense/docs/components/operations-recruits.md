---
type: View
id: operations-recruits
title: "Survivor recruitment and orders"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Survivor recruitment and orders

Recruit from liberated countries and issue legal single-defense or country-operation orders.

- **Entry, exit and actions:** Choose an accessible order country, "Recruit survivor · N coins" when legal (it opens the Recruit modal on that country's offer), then deploy a ready recruit or start Liberate country when unlocked.
- **State boundary:** Core gates enforce liberation thresholds, wallet, roster size, status and reservations. Country operations chain real defenses without automatic purchases/training.
- **Responsiveness and accessibility:** Locked milestones and disabled recruitment reasons are written; each roster entry names status and origin.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

