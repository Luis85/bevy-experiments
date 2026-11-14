---
type: View
id: equipment
title: "Equipment"
parent: preparation
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::armory"
---

# Equipment

Manage owned items for the selected personal or NPC loadout.

- **Entry, exit and actions:** Enter through Prepare, a briefing link or recovered loot; select a defender, filter and select items, compare, explicitly equip or review a sale. Escape closes an open sale confirmation first, then goes to Camp home. The Auto-sell rule (`equipment-auto-sell`) is a collapsed section under Manage equipment.
- **State boundary:** Owned instances, reservations, locks and loadouts are authoritative. Search, sorting and selected item are view state.
- **Responsiveness and accessibility:** Inventory and details split above 920 UI points; compact layouts use selected actions and an expandable full-loadout comparison.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

