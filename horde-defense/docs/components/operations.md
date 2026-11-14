---
type: View
id: operations
title: "Operations"
parent: camp-home
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Operations

Oversee actual simultaneous fields, recruit orders, garrisons and outbreaks.

- **Entry, exit and actions:** A drill-down under Camp home ("Open operations ›", "+N more in Operations", the header's plague alert and the expedition callouts), with the breadcrumb "‹ Camp home · Operations"; Escape closes an open observer window first, then goes back to Camp home. Return to active defense, observe a field, order survivors or plan an outbreak response.
- **State boundary:** Strategy clock, live battles and roster belong to Game. observed field ID and selected order country are UI state.
- **Responsiveness and accessibility:** The page scrolls and wraps orders; a written running/paused world-clock label explains every field's time state.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Fresh advanced controls require the learned feature node and applicable liberation milestone; legacy capabilities remain. Mara/recruit advancement retains its individual rules budget and is not bought through personal body nodes.
