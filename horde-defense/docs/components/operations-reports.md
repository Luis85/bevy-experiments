---
type: View
id: operations-reports
title: "Completed expedition reports"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Completed expedition reports

Review settled expedition results without paying rewards again.

- **Entry, exit and actions:** Expand Completed expedition reports after a battle settles; collapse to leave.
- **State boundary:** Read-only stored battle reports show outcome, kills and earned coins; display creates no reward transactions.
- **Responsiveness and accessibility:** Each report includes expedition ID, country and defense in text for unambiguous identification.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

