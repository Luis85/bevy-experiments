---
type: View
id: debrief-combat-log
title: "Live authoritative combat log"
parent: battle-management
status: Implemented
source: "crates/horde_game/src/ui/battle/details.rs::field_details"
---

# Live authoritative combat log

Inspect already-resolved live combat decisions through Management → Combat log.

The written log is read-only; reopening/scrolling cannot reroll an action or change injury. It belongs to active field details now, rather than a removed post-round disclosure. Historical outcomes/context remain available in Run analysis. The retained ID comes from the earlier debrief route.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
