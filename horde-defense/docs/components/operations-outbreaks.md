---
type: View
id: operations-outbreaks
title: "Urgent outbreak responses"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Urgent outbreak responses

Show active plague pressure, remaining response time and the affected front.

- **Entry, exit and actions:** Enter via alerts or Operations; Plan outbreak response selects/enters that country for a defense or survivor order.
- **State boundary:** Outbreak strength, deadline and resolution use the strategy clock. Merely viewing the alert does not contain an outbreak.
- **Responsiveness and accessibility:** Country, group count and seconds remaining accompany warning color; global pause freezes the deadline.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

