---
type: View
id: debrief-repeat
title: "Continue or leave the journey"
parent: journey-upgrades
status: Implemented
source: "crates/horde_game/src/ui/results.rs::results, actions; crates/horde_core/src/sim/journey.rs::continue_journey, leave_journey"
---

# Continue or leave the journey

Choose the next main round explicitly after any settled outcome.

Continue journey advances the reported country frontier after victory or retries its stage after defeat/withdrawal, retaining mode and making no purchases. Country liberation opens the map's eligible fronts. Leave journey pauses and saves before returning to menu, retaining progression. Main rounds do not run a hidden finite-repeat batch. The stable ID is retained from the earlier NPC-repeat surface.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
