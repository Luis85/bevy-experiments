---
type: View
id: operations-event-log
title: "Strategic event log"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Strategic event log

Inspect authoritative strategic events in reverse chronological order.

- **Entry, exit and actions:** Expand Strategic event log; collapse to return to orders.
- **State boundary:** Read-only strategy log; no simulated event is triggered by expansion or scrolling.
- **Responsiveness and accessibility:** Text history complements transient alerts and can be read at the player's pace.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

