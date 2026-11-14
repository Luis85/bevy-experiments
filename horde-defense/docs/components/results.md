---
type: View
id: results
title: "Results"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/results.rs::results, actions, results_primary"
---

# Results

The debrief every settled main round ends on.

- **Routes/actions:** Main victory, defeat or withdrawal enters. One bottom action bar holds exactly three buttons in fixed positions: Upgrade (opens Upgrades on the recommended node), the continuation (Try again, Next defense, Choose next country or Return to active defense) and a quiet Leave journey. The amber primary is Upgrade after a defeat or withdrawal whose recommended node can be bought now, otherwise the continuation. Leave journey is disabled, not hidden, over a live defense, so the bar always has three buttons. Escape opens the return-to-menu confirmation.
- **State:** Core settlement owns rewards and history; the screen pays nothing again. Inline content above the bar is the outcome strip (" · <name>" for a defender other than you, then the level-grant and milestone lines), the loot block (`results-loot-block`), the liberation recruit block that opens the Recruit modal (`recruit-modal`), the next-defender picker (`defender-picker`) and the muted count of affordable upgrades. Its only controls are that tested inline whitelist.
- **Layout:** The global LASTLIGHT header (wallet, Menu, Save, History, Settings) and no camp row. Drawn without a settled report it hands over to Camp home.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
