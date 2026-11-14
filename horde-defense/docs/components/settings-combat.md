---
type: View
id: settings-combat
title: "Aiming and firing settings"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::combat_settings"
---

# Aiming and firing settings

Adjust aim assistance, automatic empty-magazine reload and toggle firing.

- **Entry, exit and actions:** Change assistance/checkboxes; save through the settings return action.
- **State boundary:** Settings alter input assistance; runtime still validates fire cadence and action legality.
- **Responsiveness and accessibility:** Toggle firing reduces hold requirements. Written guidance explains that melee interrupts unfinished reloads and that Management lists Reload, Strike and Toggle firing in a defense.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

