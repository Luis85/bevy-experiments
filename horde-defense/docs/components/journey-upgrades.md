---
type: View
id: journey-upgrades
title: "Journey upgrade home"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/results.rs::results"
---

# Journey upgrade home

Every settled main round returns to the same upgrade home.

- **Routes/actions:** Main victory, defeat or withdrawal enters; inspect the already-banked outcome, choose graph upgrades, open Equipment/Run analysis or unlocked More routes, then Continue journey or Leave journey.
- **State:** Core settlement owns rewards/history. The whole graph reads actual wallet, ownership and requirements; selecting or browsing buys nothing. Parallel completions retain their separate orchestration.
- **Layout:** The compact outcome/actions lead one connected node canvas and selected purchase details. There are no separate personal body/skill/fortification purchase tabs.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.

This screen owns its single heading/wallet/Settings without the duplicated global header. Failed-round rest/train wording is shown once. The named continuation action reflects retry/next defense/country choice.
