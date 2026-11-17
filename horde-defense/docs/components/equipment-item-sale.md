---
type: View
id: equipment-item-sale
title: "Single item sale review"
parent: equipment-details
status: Implemented
source: "crates/horde_game/src/ui/armory.rs::item_sale_confirmation"
---

# Single item sale review

Review the coin return of an item nothing holds. Selling is strict (W1-20): an item that is equipped, in a preset, reserved, protected, locked, a favorite or NPC-issue cannot be sold (nor anything while a defender is deployed), and a sale never rewrites a loadout or a preset.

- **Entry, exit and actions:** `Review sale · <coins>` opens the review for a sellable item; Confirm item sale dispatches `Command::SellItem`; Keep item cancels. If the item became unsellable after the review opened, the review shows the reason and Dismiss sale.
- **State boundary:** `Game::sale_eligibility` gives a `SaleQuote` (the coins) or a typed `SaleBlock`; the core `SellItem` command applies the same rule. pending_sale is transient selection state.
- **Responsiveness and accessibility:** A blocked item's Review sale button is disabled, and its hover text and the review state the first blocking reason in the core's words (`SaleBlock::label()`), for example the presets that use it. The review never lists affected loadouts, because a sellable item has none.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

