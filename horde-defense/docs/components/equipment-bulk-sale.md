---
type: View
id: equipment-bulk-sale
title: "Bulk equipment sale review"
parent: equipment
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::sale_confirmation"
---

# Bulk equipment sale review

Sell unused unprotected extras after a current quote and explicit confirmation.

- **Entry, exit and actions:** Sell unprotected extras reveals the quote; Confirm sale commits; Cancel closes the review.
- **State boundary:** Game computes exclusions and transaction value; equipped, protected and retained preset references follow core rules.
- **Responsiveness and accessibility:** A written summary and distinct Confirm/Cancel buttons make the scope visible.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

