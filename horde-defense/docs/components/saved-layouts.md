---
type: View
id: saved-layouts
title: "Country plans during live construction"
parent: battle-build
status: Implemented
source: "crates/horde_game/src/ui/battle.rs::construction_controls"
---

# Country plans during live construction

Save or clear the active country's defensive intention during construction.

Save country plan captures surviving structures; Clear country plan affects only the stored country plan, not deployed structures. Ordinary and observed fields expose their actual country context. Plans remain ordered and deployment spends renewed supply only for legal affordable placements, reporting omissions without deleting the full plan. Opening/using controls does not pause; explicit pause remains separate.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
