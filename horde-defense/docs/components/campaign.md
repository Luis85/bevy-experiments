---
type: View
id: campaign
title: "Campaign world map"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/camp.rs::world_ui"
---

# Campaign world map

Choose and inspect the country to defend, showing liberation progress and reachable fronts.

- **Entry, exit and actions:** Enter from main-menu, preparation or debrief; map selection opens the country dossier; start/enter actions use runtime commands.
- **State boundary:** World records and campaign access rules are authoritative; search, selected country and map camera are view state.
- **Responsiveness and accessibility:** Map and dossier sit side by side above900 available points; compact mode reserves the map first and collapses the dossier below it. The country browser provides a text alternative to map selection.

**Current increment contract:** First-country confirmation immediately creates and enters a live personal attempt. Later country entry opens briefing; preparation remains reachable for deliberate changes.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

