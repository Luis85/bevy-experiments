---
type: View
id: observed-battle
title: "Observed expedition battlefield"
parent: operations
status: Implemented
source: "crates/horde_game/src/ui/operations.rs::operations"
---

# Observed expedition battlefield

Watch an actual autonomous expedition and inspect its actors or structures.

- **Entry, exit and actions:** Observe a live expedition opens its bounded window; close it to return to Operations; a settled battle points back to its report.
- **State boundary:** The chosen battle's Attempt is authoritative; observer window/aim are presentation. Viewing never supplies manual shots for its NPC.
- **Responsiveness and accessibility:** Window dimensions clamp to the viewport and hover details remain available; HP, action and supply are written above the arena.

**Current increment contract:** Observed construction remains live: place/salvage actions do not implicitly pause the field or strategic clock. Pause all is an explicit separate action.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

