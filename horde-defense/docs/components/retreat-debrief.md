---
type: View
id: retreat-debrief
title: "Settled main-round outcome"
parent: journey-upgrades
status: Implemented
source: "crates/horde_game/src/ui/results.rs::results"
---

# Settled main-round outcome

Read the last settled round before choosing the next action.

The outcome strip names victory/defeat/retreat, country/stage and already-banked coins/items. It does not pay again or independently deploy. Every outcome shares the Results screen; the retained ID identifies this informational portion of the former debrief surface, and its parent ID is retained from the time Results and the upgrade tree were one screen.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
