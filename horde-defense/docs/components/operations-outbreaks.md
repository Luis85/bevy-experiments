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

- **Entry, exit and actions:** Enter via Camp home's alerts or Operations; "Plan response" dispatches `SelectCountry` and opens the Briefing on the outbreak's final stage. Nothing deploys until Deploy (`BeginDefense`); survivor orders stay available.
- **State boundary:** Outbreak strength, deadline and resolution use the strategy clock. Merely viewing the alert does not contain an outbreak.
- **Responsiveness and accessibility:** Country, group count and seconds remaining accompany warning color; global pause freezes the deadline.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

