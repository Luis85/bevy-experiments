---
type: View
id: equipment-inventory
title: "Owned equipment inventory"
parent: equipment
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::inventory_list"
---

# Owned equipment inventory

Browse owned equipment without changing the loadout on selection.

- **Entry, exit and actions:** Search name or rarity, filter slot, sort Newest/Name/Tier and select an item; selection opens equipment-details.
- **State boundary:** Stable item IDs identify instances with saved acquisition levels; UI sorting never changes ownership, levels or authoritative ordering.
- **Responsiveness and accessibility:** Rows show the actual instance level and NPC-issued labels where applicable; the list scrolls and reports matching/total counts. Empty results explain how to clear the filter.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Quality is Poor/Common/Uncommon/Rare/Epic/Legendary, separately named and colored from item Lv and unlock tier. Class-specific icons have quality borders; actual effective comparisons/value use quality and saved level. Missing/base quality preserves old numerical identity. Inventory supports quality filtering/sorting; shop offers fixed base quality rather than a lottery.
