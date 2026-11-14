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

- **Routes/actions:** Main victory, defeat or withdrawal enters. One bottom action bar holds exactly three buttons in fixed positions: Upgrade (opens Upgrades on the recommended node), the continuation (Try again, Next defense, Choose next country or Return to active defense) and a quiet Leave journey. The amber primary is Upgrade after a defeat or withdrawal whose recommended node can be bought now, otherwise the continuation. Escape asks before returning to the main menu.
- **State:** Core settlement owns rewards and history; the screen pays nothing again. Inline content above the bar is the outcome strip, the loot block, the liberation recruit block, the next-defender picker and the muted count of affordable upgrades.
- **Layout:** The global LASTLIGHT header (wallet, Menu, Save, History, Settings) and no camp row. Drawn without a settled report it hands over to Camp home.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
