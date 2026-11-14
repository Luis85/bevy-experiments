---
type: View
id: journey-upgrades
title: "Journey upgrade home"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/upgrades.rs::upgrades, actions, upgrades_continue"
---

# Journey upgrade home

The Upgrades screen: the one route to the complete tree.

- **Routes/actions:** Results' Upgrade, the camp row's Journey upgrades entry, Camp home's upgrade offer and the retired Skills and Fortifications tabs open it. The camp navigation row (Camp, Journey upgrades selected, Briefing, Equipment, Character, World map) sits above the page. The action strip is the amber continuation (Back to battle, Choose next country, or Return to active defense when a main attempt is live) and Back to camp; with no settled round only Back to camp is shown, as the primary. Escape goes back to Results when a round has settled, otherwise to Camp home.
- **State:** The whole graph reads actual wallet, ownership and requirements; selecting or browsing buys nothing. Back to battle dispatches the same continuation as Results (the lost stage again, or the next defense after a win). Fields stay paused while the screen is open.
- **Layout:** The strip leads one connected node canvas at full height with the selected purchase details. There are no separate personal body/skill/fortification purchase tabs.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.

This screen uses the same global LASTLIGHT header as the camp (wallet, Menu, Save, History, Settings) and no second wallet. The settled outcome, its loot and the Leave journey action are on [Results](results.md).
