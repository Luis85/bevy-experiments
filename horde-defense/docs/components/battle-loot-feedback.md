---
type: View
id: battle-loot-feedback
title: "Already-banked loot feedback"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/loot.rs::draw, draw_for"
---

# Already-banked loot feedback

Recognize a successful drop without collecting or awarding it again.

- **Entry, exit and actions:** A model receipt appears at the actual defeated actor position as a quality-outlined icon/name, rises and fades automatically over 1.5 real UI seconds. HUD/debrief receipts retain the already-banked explanation.
- **State boundary:** LootEvent is emitted after authoritative inventory insertion. The cache keys battle/attempt/item identity, prioritizes quality and keeps at most twelve active flyups per stream. It grants no items/coins and consumes no combat randomness. Capture-only replay changes this presentation cache without mutating Game.
- **Responsiveness and accessibility:** Explicit quality names supplement all six border colors; Rare+ can use a bounded existing cue. Real-time duration remains readable at 4× observation. Actual Enemy.y() and the deployment-frozen arena projection locate the receipt.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim an OS playthrough.
