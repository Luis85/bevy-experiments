---
type: View
id: equipment-presets
title: "Loadout presets"
parent: equipment
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::armory"
---

# Loadout presets

Save and reapply references to owned equipment.

- **Entry, exit and actions:** Expand Loadout presets, name and save the current loadout, Use a listed preset, or Delete a listed preset (`Command::DeletePreset`; its items stay in the armory and become sellable unless something else holds them); collapse to leave.
- **State boundary:** Presets reference unique instances; applying one cannot duplicate items or bypass reservations. Name input is presentation state.
- **Responsiveness and accessibility:** Controls wrap, and help text explains why referenced items survive bulk sales.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

