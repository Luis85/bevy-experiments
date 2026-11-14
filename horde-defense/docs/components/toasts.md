---
type: View
id: toasts
title: "Toasts"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/toast.rs::toast_layer, battle_toast, status_toast, TOAST_SECS, TOAST_ERROR_SECS; crates/horde_game/src/ui/expedition_notices.rs::deliver, outcome_line, folded_line"
---

# Toasts

Short feedback that never blocks and never takes focus.

- **Placement:** On every screen without a battle field, up to three toasts stack top-right under the header (offset 16 by 52 points, 280 points wide), newest first; a long line wraps. A battle shows only the newest, as one truncated line (full text on hover) in `BattleLayout.receipt_slot`, where it takes the slot from the loot receipt.
- **Sources:** "Saved" after a manual save ("Saved · recovery copies kept" for 6 s when copies were kept), "Sold N · +C" after a stash apply, battle status messages (the battle has no status line; an error is an Error toast), and expedition outcomes: one toast per settled expedition from `Game::unacknowledged_outcomes` ("<Recruit> liberated <Country> · N new fronts", or what the recruit did there and its coins), each acknowledged with `Command::AcknowledgeOutcomes` in the same frame, plus one "N more expedition outcomes · Operations" for folded ones. Outcomes wait while a main defense is live and unpaused, and outside the journey's screens. No toast mentions auto-sell.
- **Timing:** Info and Success toasts last 3 s, Error toasts 6 s; the same text arriving again restarts its timer instead of stacking, a hovered toast is held, and the queue keeps at most eight. The clock is the egui input clock (wall time), never the battle tick. Normal effects fades the last 0.25 s; reduced effects has no fade.
- **State boundary:** `StatusUi.toasts` is presentation state and is never saved; acknowledging an outcome is the only command a toast causes.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
