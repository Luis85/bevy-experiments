---
type: View
id: battle-loot-feedback
title: "Already-banked loot feedback"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/loot.rs::draw, draw_for, glint_phase, glint_band, marker_center; crates/horde_game/src/ui/chrome.rs::paint_quality_shape"
---

# Already-banked loot feedback

Recognize a successful drop without collecting or awarding it again.

- **Entry, exit and actions:** A model receipt appears at the actual defeated actor position as a quality-outlined icon/name and fades automatically over 1.5 real UI seconds. In normal effects it rises (and drifts sideways by item id) while it fades; in reduced effects it holds where it appears, 12 pt over the kill, with no rise and no drift, and only fades. HUD/debrief receipts retain the already-banked explanation.
- **State boundary:** LootEvent is emitted after authoritative inventory insertion. The cache keys battle/attempt/item identity, prioritizes quality and keeps at most twelve active flyups per stream. It grants no items/coins and consumes no combat randomness. Capture-only replay changes this presentation cache without mutating Game.
- **Responsiveness and accessibility:** Explicit quality names supplement all six border colors (Rare and better also name the item and draw a 2 pt border). Every tier carries a static corner marker, chrome's quality shape on an INK disc inside the icon's top-right corner, the same in reduced effects, so quality never rests on colour alone. Rare and better get one diagonal glint across the icon between 0.15 and 0.55 s; reduced effects draws no glint. The Rare+ drop sound is the runner's (`horde_audio::loot::LootTracker`), not this view's. Real-time duration remains readable at 4× observation. Actual Enemy.y() and the deployment-frozen arena projection locate the receipt.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim an OS playthrough.
