---
type: View
id: main-menu
title: "Main menu"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/menu.rs::menu"
---

# Main menu

Resume the local campaign or choose a personal survivor before its first country.

- **Entry, exit and actions:** Launch or saved return enters; a fresh New game route opens survivor confirmation, then the world-country choice. Continue opens the campaign or active battle. Change survivor is available before the first country; Settings and Save and quit remain explicit actions.
- **State boundary:** Committed identity, profile existence and active attempt come from Game. The preview identity, measured title, keyart-v2 checkpoint layering and animation are presentation; menu motion consumes no simulation RNG or ticks.
- **Responsiveness and accessibility:** The title and actions use compact spacing and a scrollable content area on short windows; reduced effects freezes the checkpoint scene and save errors remain visible.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


The runtime preview uses the original source-image cabin foot anchor (0.400, 0.807), at 21% source-image height, transformed with the same background crop/parallax. Seven active atlases use cached managed premultiplied-alpha conversion and nearest sampling; source PNGs are unchanged.

The selected menu figure uses menu-survivors-v1.png as one coherent low-ready-pistol pose: hands/arms/pistol breathe together above planted boots. The original background anchor/21% height remains; battle and Character preview resolve the actual frozen loadout through the source-owned player_model facade.

Existing-profile New game uses a protective confirmation/archive service. Resume routes to the active field, the settled journey upgrade home or incomplete selection. Leaving and New game are distinct actions.
