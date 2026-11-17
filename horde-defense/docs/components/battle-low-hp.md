---
type: View
id: battle-low-hp
title: "Low-HP warning"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/low_hp.rs::low_hp, low_hp_edge, edge_alpha, hp_text, LOW_HP_ENTER_PCT, LOW_HP_EXIT_PCT"
---

# Low-HP warning

Make a defender close to falling impossible to miss, in colour and in text.

- **Entry, exit and actions:** At 30% of maximum HP or less the defender is low; it stays low until HP is back above 35% (hysteresis, so it never flickers at the threshold). While low, an 8-point red band lies inside all four canvas edges, painted after the field and before the HUD frames, and the defender HUD's HP text reads "<name> · HP h / m · LOW". It applies to the personal defender and to Mara or a recruit holding the main defense, not to an observed expedition.
- **State boundary:** `low_hp(hp, max_hp, was_low)` is pure integer arithmetic over the HUD's values; the flag is `BattleUi.low_hp`. No sound depends on it, and it changes nothing in the simulation.
- **Responsiveness and accessibility:** Normal effects pulses the band between 0.25 and 0.50 strength once a second while the battle runs; reduced effects and a paused battle hold it at a static 0.40. The "LOW" text means the warning never rests on colour alone.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
