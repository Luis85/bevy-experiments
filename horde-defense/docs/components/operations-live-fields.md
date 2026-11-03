---
type: View
id: operations-live-fields
title: "Live expeditions"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Live expeditions

Summarize active autonomous battles and their current decision.

- **Entry, exit and actions:** Select Observe to open an observed battlefield or Recall to end an expedition while retaining earned rewards.
- **State boundary:** Summaries read real battle IDs, HP, elapsed time, kills and decisions. Recall is a validated command.
- **Responsiveness and accessibility:** Each field names survivor, country and defense; empty state explains deployment prerequisites.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

