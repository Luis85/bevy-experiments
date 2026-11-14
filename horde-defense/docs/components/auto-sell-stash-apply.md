---
type: View
id: auto-sell-stash-apply
title: "Apply auto-sell to the stash"
parent: equipment-auto-sell
status: Implemented
source: "crates/horde_game/src/ui/modal.rs::show_auto_sell, auto_sell_refused, auto_sell_sold, auto_sell_title, auto_sell_confirm"
---

# Apply auto-sell to the stash

Confirm one sale of exactly the quote the player saw.

- **Entry, exit and actions:** "Apply rule to stash now…" opens it. The title asks "Sell N items for C coins?", the body names what is kept (locked, favourite, equipped, preset, recruit and shop items) and the quote by quality. The amber "Sell N · +C" dispatches `Command::ApplyAutoSellToStash` for that quote and waits for the runner; "Keep items", Escape and a backdrop click close it and sell nothing. A finished sale closes the modal and toasts "Sold N · +C".
- **State boundary:** Core refuses a stale quote (`QuoteChanged`), a full wallet or a defense that began meanwhile; the modal then shows the quote core would accept now, says "The stash changed. This is the new quote." when it differs, and closes when nothing is left to sell. The status line carries core's reason.
- **Responsiveness and accessibility:** One modal slot; the primary is disabled while the confirmation is with the runner.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
