---
type: View
id: operations
title: "Operations"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Operations

Oversee actual simultaneous fields, recruit orders, garrisons and outbreaks.

- **Entry, exit and actions:** Enter Operations from camp navigation or alerts; return to active defense, observe a field, order survivors or plan an outbreak response.
- **State boundary:** Strategy clock, live battles and roster belong to Game. observed field ID and selected order country are UI state.
- **Responsiveness and accessibility:** The page scrolls and wraps orders; a written running/paused world-clock label explains every field's time state.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Fresh advanced controls require the learned feature node and applicable liberation milestone; legacy capabilities remain. Mara/recruit advancement retains its individual rules budget and is not bought through personal body nodes.
