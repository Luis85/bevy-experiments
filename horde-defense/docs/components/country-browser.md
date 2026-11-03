---
type: View
id: country-browser
title: "Country browser"
parent: campaign
status: Implemented
source: "crates/horde_game/src/ui/camp.rs::country_results"
---

# Country browser

Find countries by name or code and browse available fronts.

- **Entry, exit and actions:** Typing reveals matching results; otherwise expand Browse available countries. Select a result to update the dossier; clear search to return to ordinary browsing.
- **State boundary:** Filtering changes selection only; selecting a locked country cannot grant access or spend resources.
- **Responsiveness and accessibility:** Results wrap and scroll within a bounded area. Names remain readable alongside map color states.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Compact query results use a bounded28-unit viewport (112 ordinarily), preserving the map projection/controls before the optional dossier. Text selection remains a non-transactional alternative to the map.
