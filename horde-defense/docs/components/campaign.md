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

- **Entry, exit and actions:** Enter from main-menu, the camp row's World map, Results' Choose next country or Camp home's Choose a country; map selection opens the country dossier. The footer's amber primary is "Start journey" for the first country (`ChooseStart`), then "Plan defense" or, once liberated, "Manage country" (`SelectCountry`, then the Briefing). Escape goes to the main menu before the first start and to Camp home after it.
- **State boundary:** World records and campaign access rules are authoritative; search, selected country and map camera are view state.
- **Responsiveness and accessibility:** Map and dossier sit side by side above900 available points; compact mode reserves the map first and collapses the dossier below it. The country browser provides a text alternative to map selection.

**Current increment contract:** Start journey creates the first personal attempt paused, with the first-defense hint and the key legend; Begin defense starts it. A later country is chosen without deploying or spending an attempt id, and the Briefing's Deploy (`BeginDefense`) starts its defense. The dossier names the defenses a country needs and what liberating it opens.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

