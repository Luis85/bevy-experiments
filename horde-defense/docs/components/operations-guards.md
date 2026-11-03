---
type: View
id: operations-guards
title: "Country guards"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Country guards

Assign a ready survivor to guard the selected country or release an existing guard.

- **Entry, exit and actions:** Guard selected country is available after its strategic unlock; Release guard recalls the assigned survivor.
- **State boundary:** Guard duty and country validity are core rules. Assigned unique equipment stays reserved for the duty.
- **Responsiveness and accessibility:** The roster shows Guarding plus the country name, independently of color.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

