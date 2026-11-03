---
type: View
id: country-conditions
title: "Country conditions"
parent: country-dossier
status: Implemented
source: "crates/horde_game/src/ui/camp.rs::country_summary"
---

# Country conditions

Disclose population and authored boosts, buffs and debuffs without crowding the map.

- **Entry, exit and actions:** Expand Country conditions and population; collapse it to return to the compact dossier. No campaign mutation occurs.
- **State boundary:** Read-only authored country content; expansion state belongs to egui.
- **Responsiveness and accessibility:** Benefits and penalties retain written labels and do not depend on green/red alone.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

