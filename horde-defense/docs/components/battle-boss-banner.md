---
type: View
id: battle-boss-banner
title: "Boss banner"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle/boss_banner.rs::boss_banner, banner_step, banner_alpha, banner_text, BOSS_BANNER_SECS, BOSS_ARRIVED_SECS"
---

# Boss banner

The visual twin of the boss warning and arrival sounds.

- **Entry, exit and actions:** When the director warns, "BOSS APPROACHING" with the subline "<boss> · arrives in N s" shows for 2 s; when the boss is on the field, "<BOSS> HAS ARRIVED" shows for 1.5 s. It is 20-point amber type on the panel colour, `min(380, 0.6 x arena width)` by 34 points, centred on the arena 6 points under its top (4 points under the hint callout while that shows), and it ends left of an open construction panel. The HUD's "BOSS IN mm:ss" stays. Observed expeditions show it in their own window.
- **State boundary:** Its clock is the egui input clock and counts only frames in which the battle runs on the battle screen, at most 0.1 s a frame: a paused battle holds it, and 1×, 2× and 4× observation show it equally long. A warning more than 2 s of battle time old when first seen starts no banner. `battle_view::BossNotice` is a pure core read; nothing here touches the simulation.
- **Responsiveness and accessibility:** Normal effects fades it in over 0.15 s and out over 0.30 s; a paused frame skips the fade-in, so a banner first seen, or caught fading in, while the battle is paused shows at full strength in both modes; reduced effects shows it at full strength for its whole life. Long names shrink to 9 points before they are clipped.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
