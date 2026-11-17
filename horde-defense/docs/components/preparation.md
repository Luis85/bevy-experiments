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

- **Entry, exit and actions:** One navigation row of six entries: Camp (Camp home, the default tab), Journey upgrades (while no main attempt is live; it opens the Upgrades screen), Briefing, Equipment, Character and World map. Mara and Operations are drill-downs under Camp home with a "‹ Camp home" breadcrumb, and the row keeps Camp selected on them. The footer primary is Camp home's `home_primary` on Home, Deploy on Briefing ("Deploy personally" or "Deploy <name>"), the World footer's Start journey / Plan defense / Manage country on the map, and "Go to briefing" (or "Choose next country" after a liberation) on the other Prepare pages; "Return to active defense" replaces it while a main attempt is live. Only Camp home shows the world clock (Resume world clock / Pause world clock, key P by default). Escape on any camp tab but Home goes to Camp home; the observer window and armory sale confirmations close first. An unlock callout may sit under the row.
- **State boundary:** Tabs, remembered tab and recommendation focus are scene state. Purchases, training and deployment are typed runtime commands.
- **Responsiveness and accessibility:** Ordinary tabs scroll vertically with persistent mission actions; the row wraps at narrow widths and large text scales instead of adding a second row.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Camp home is the camp's front page; the upgrade tree is the separate Upgrades screen. Preparation remains an explicit Equipment/stats or progressively revealed management destination; personal body/skills/fortifications do not use separate purchase screens. The retired Skills and Fortifications tabs redirect to Upgrades; Fortifications frames the Fortification branch.
