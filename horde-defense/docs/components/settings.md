---
type: View
id: settings
title: "Settings"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::settings_ui"
---

# Settings

Adjust next-run gameplay balance, readability, controls and audio while all fields are paused.

- **Entry, exit and actions:** Open from menu, camp, Results, Upgrades or battle; Save and return names its destination; Escape follows that route, cancels a rebind prompt first, and opens the unapplied-draft warning (`draft-warning`) while the difficulty draft is not applied; an active game also offers Save and return to main menu.
- **State boundary:** Readability/input/audio are local preferences; applied gameplay balance is validated campaign state for future deployments, while its unapplied draft is UI state. Return screen/prior clock state are view context; durable save acknowledgment governs exit and errors remain visible.
- **Responsiveness and accessibility:** Groups use two columns above 800 UI points and stack below that width, within a vertical scroll area.

**Current increment contract:** Settings explicitly pauses fields and preserves its durable return contract. Opening live construction does not invoke this pause.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Queued/in-flight durable transitions freeze both raw preference/rebinding edits and later model commands. An older autosave acknowledgment cannot clear a newer critical save busy state; named return/quit waits for its matching durable acknowledgment.
