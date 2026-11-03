---
type: View
id: operations-recruit-details
title: "Recruit equipment training and policy"
parent: operations-recruits
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Recruit equipment training and policy

Prepare an individual ready recruit with independent equipment, training and policy.

- **Entry, exit and actions:** Expand a ready recruit's Equipment, training & policy; buy/allocate CP, equip available instances or change policy; collapse to leave.
- **State boundary:** Commands identify the recruit and item by stable IDs; advancement allowance and active reservations remain authoritative.
- **Responsiveness and accessibility:** Each disclosure includes available CP, stats and item reservation labels; controls wrap in the scrolling roster.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

