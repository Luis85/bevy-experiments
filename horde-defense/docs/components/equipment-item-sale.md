---
type: View
id: equipment-item-sale
title: "Single item sale review"
parent: equipment-details
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::item_sale_confirmation"
---

# Single item sale review

Review a sale's coin return and effects on referenced loadouts or presets.

- **Entry, exit and actions:** Review sale opens; Confirm item sale dispatches the sale; Keep item cancels; invalid quotations show a dismissal action.
- **State boundary:** The core sale preview supplies the current quote and affected references. pending_sale is transient selection state.
- **Responsiveness and accessibility:** Affected presets/survivors and invalid-sale reasons appear in text before confirmation.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

