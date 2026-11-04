---
type: View
id: preparation
title: "Preparation"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/camp.rs::camp; crates/horde_game/src/ui/navigation.rs::camp_navigation, camp_entries; crates/horde_game/src/ui/briefing.rs::mission_actions"
---

# Preparation

Organize optional improvements and subsequent deployment decisions.

- **Entry, exit and actions:** One navigation row: Journey upgrades (while no main attempt is live; it opens the upgrade home), Briefing, Equipment, Character, Mara (when unlocked), World map and Operations (when unlocked). Every page reaches the home in one click. The footer primary is Deploy on Briefing, "Go to briefing" on the other Prepare pages and "Return to active defense" while a main attempt is live; it is the only return route on the page. The clock control reads Resume world clock / Pause world clock; its hover notes when resuming also returns to the live field.
- **State boundary:** Tabs, remembered tab and recommendation focus are scene state. Purchases, training and deployment are typed runtime commands.
- **Responsiveness and accessibility:** Ordinary tabs scroll vertically with persistent mission actions; the row wraps at narrow widths and large text scales instead of adding a second row.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


The incremental primary home is Journey upgrades. Preparation remains an explicit Equipment/stats or progressively revealed management destination; personal body/skills/fortifications do not use separate purchase screens. The retired Skills and Fortifications tabs redirect to the home; Fortifications frames the Fortification branch.
