---
type: View
id: preparation
title: "Preparation"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/camp.rs::camp, upgrade_surface"
---

# Preparation

Organize optional improvements and subsequent deployment decisions.

- **Entry, exit and actions:** Prepare remembers the last preparation tab; choose Briefing, Equipment, Your character, Mara, Skills or Fortifications; return to Campaign or Operations.
- **State boundary:** Tabs, remembered tab and recommendation focus are scene state. Purchases, training and deployment are typed runtime commands.
- **Responsiveness and accessibility:** Ordinary tabs scroll vertically with persistent mission actions; graph tabs devote most available space to their canvas.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


The incremental primary home is Journey upgrades. Preparation remains an explicit Equipment/stats or progressively revealed management destination; personal body/skills/fortifications do not use separate purchase screens.
