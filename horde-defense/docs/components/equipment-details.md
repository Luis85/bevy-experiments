---
type: View
id: equipment-details
title: "Equipment details and comparison"
parent: equipment
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::selected_item, selected_actions, inventory_comparison"
---

# Equipment details and comparison

Compare a selected item against the defender's current complete loadout before committing.

- **Entry, exit and actions:** Select an owned item; Equip selected, Lock, Favorite or Review sale are explicit actions; another selection replaces the comparison.
- **State boundary:** Comparisons read complete campaign profiles using the selected item instance’s saved level and effective values. Runtime validates item ownership and reservations; viewing never equips automatically.
- **Responsiveness and accessibility:** Compact layouts expose the comparison through a labeled disclosure. Reserved items show a written reason and recall guidance.

**Current increment contract:** Personal ranged equipment requires ownership and trained class eligibility. NPC-issued gear is labeled separately and unavailable to fresh personal loadouts; compatible legacy access is explained explicitly. Innate pistol training never grants additional items. Purchase and equipment selection remain explicit; the on-field selector chooses the equipped gun or an unlocked throwable.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Quality is Poor/Common/Uncommon/Rare/Epic/Legendary, separately named and colored from item Lv and unlock tier. Class-specific icons have quality borders; actual effective comparisons/value use quality and saved level. Missing/base quality preserves old numerical identity. Inventory supports quality filtering/sorting; shop offers fixed base quality rather than a lottery.

Actual equipped appearance is snapshotted by definition/level/quality/style/class into the immutable defender profile. Previewing or selecting an owned item does not equip it, spend currency or change rules.
