---
type: View
id: country-dossier
title: "Country dossier"
parent: campaign
status: Implemented
source: "crates/horde_game/src/ui/camp.rs::country_summary, country_action"
---

# Country dossier

Summarize the selected country's defenses, difficulty, biome and liberation progress.

- **Entry, exit and actions:** Map or browser selection updates the dossier; Choose survivor first routes an unselected profile to survivor-selection; after confirmation, Start in this country chooses the first front; Plan defense enters later fronts; Return to active defense restores an existing attempt.
- **State boundary:** The world catalog supplies country properties; Game validates access. Selection never starts a second active defense implicitly.
- **Responsiveness and accessibility:** Compact dossier is a named disclosure below a reserved map-first region, rather than above a tiny map strip. Numerical progress and labels accompany color.

**Current increment contract:** After confirming the personal survivor, Start in this country directly enters live battle, with no extra briefing or Begin defense step. Later-country selection exposes the front's progression and legal planning actions.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

