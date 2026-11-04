---
type: View
id: survivor-selection
title: "Choose your survivor"
parent: main-menu
status: Implemented
source: "crates/horde_game/src/ui/menu.rs::survivor_selection"
---

# Choose your survivor

Choose the personal survivor before selecting the campaign's starting country.

- **Entry, exit and actions:** A fresh menu enters through Choose survivor. Select Rook or June to preview; Continue as the named survivor confirms the choice and opens country selection. Back to main menu returns without committing a new identity. Change survivor remains available before the starting country is committed.
- **State boundary:** CampaignUi.preview_survivor is presentation state. Command::ChooseSurvivor (via Game::apply) commits Campaign.selected_survivor; country selection requires that identity. Rook and June begin with the same attributes and pistol. Mara remains the independent NPC companion.
- **Responsiveness and accessibility:** Portraits, names, explicit selected state and a named confirmation accompany the animated preview. The compact content scrolls; reduced effects keeps the scene static.

The menu layers the chosen original low-ready figure over the checkpoint background; the same saved appearance supplies personal battlefield presentation. Legacy campaigns retain their compatible default identity rather than being forced through a new campaign.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

The runtime preview uses the original source-image cabin foot anchor (0.400, 0.807), at 21% source-image height, transformed with the same background crop/parallax. Seven active atlases use cached managed premultiplied-alpha conversion and nearest sampling; source PNGs are unchanged.

The selected menu figure uses menu-survivors-v1.png as one coherent low-ready-pistol pose: hands/arms/pistol breathe together above planted boots. The original background anchor/21% height remains; battle and Character preview resolve the actual frozen loadout through the source-owned player_model facade.

A confirmed fresh journey reaches this choice before the map; it creates no equipment advantage between Rook/June. Any old profile replacement was already protected by the new-game service.
