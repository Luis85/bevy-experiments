---
type: View
id: battle-build
title: "Live construction"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/construction.rs::construction_controls, placement_cursor"
---

# Live construction

Place owned defenses within the left-half construction cells and inspect remaining supply.

- **Entry, exit and actions:** Open Build, select a structure, place a valid preview or salvage an existing structure; close Build to resume the aiming presentation.
- **State boundary:** Placement legality, supply, salvage and saved plans belong to core rules; selected kind and cell preview are scene state.
- **Responsiveness and accessibility:** A bounded scrolling panel explains costs and invalid-placement reasons. Preview labels supplement valid/invalid color.

**Current increment contract:** Opening, placing and salvaging do not pause combat. Explicit pause remains separate. Save this country’s layout records current surviving structures during combat without granting supply; future deployments pay placement costs and apply only the ordered affordable prefix.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Construction has five columns across the actual frozen rows and left-half bounds. Shared core construction_bounds, build_cell_center and build_cell_at drive preview/input/rendering; Normal 12-yard bounds are [2,6], while legacy 30-yard bounds are [3,15]. Unavailable stored cells are omitted without spending supply or deleting the plan.
