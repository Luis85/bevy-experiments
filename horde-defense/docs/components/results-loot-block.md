---
type: View
id: results-loot-block
title: "Results loot block"
parent: results
status: Implemented
source: "crates/horde_game/src/ui/results/loot_block.rs::loot_block, UPGRADE_ROWS, AUTO_SELL_OFF"
---

# Results loot block

Show what the reported run dropped, what settlement auto-sold and which finds beat the defender's gear.

- **Content:** "Loot N" followed by the count of each quality with its marker shape and colour; the auto-sell line ("Auto-sold N · +C", with the receipt's lines on hover; "Auto-sold 0 · +0" while the rule is on and sold nothing; the muted "Auto-sell is off · Equipment › Auto-sell" while it is off); then at most two "Better than equipped:" rows (or "Better than <Name>'s gear:" for a recruit's run), largest gain first. A row for you or Mara has "Equip", which dispatches the offer's own command; a recruit's row has "Operations ›", where its gear is changed. The block is hidden when the run dropped nothing.
- **State boundary:** It renders `Game::results_loot` and nothing else: the totals, the receipt, the offers and each equip command are core's. Nothing is paid again and nothing is sold from here.
- **Responsiveness and accessibility:** Inline content above the three-button action bar, never one of its actions (it is on Results' tested whitelist of inline controls). Quality has a name and a shape as well as a colour; the block is static, so reduced effects needs no second form.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
