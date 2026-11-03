---
type: View
id: equipment-shop
title: "Quartermaster shop"
parent: equipment
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::armory"
---

# Quartermaster shop

Offer guaranteed equipment purchases supported by current progression.

- **Entry, exit and actions:** Expand Quartermaster or follow a recommendation; Buy an affordable unlocked item; compare and equip it separately in the inventory.
- **State boundary:** Catalog tiers, campaign progression, level-adjusted prices and wallet determine availability. Purchases create an item at the offered saved level through commands, without equipping it.
- **Responsiveness and accessibility:** Items show names, slots, offered levels and prices in text, with effective-stat hover details; purchase controls disable when unaffordable.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Quality is Poor/Common/Uncommon/Rare/Epic/Legendary, separately named and colored from item Lv and unlock tier. Class-specific icons have quality borders; actual effective comparisons/value use quality and saved level. Missing/base quality preserves old numerical identity. Inventory supports quality filtering/sorting; shop offers fixed base quality rather than a lottery.
