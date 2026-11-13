---
type: View
id: journey-upgrades
title: "Journey upgrade home"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/results.rs::results, actions"
---

# Journey upgrade home

Every settled main round returns to the same upgrade home.

- **Routes/actions:** Main victory, defeat or withdrawal enters; inspect the already-banked outcome, choose graph upgrades. The camp navigation row (Journey upgrades selected, Briefing, Equipment, Character, Mara, World map, Operations) sits above the page. The action row is the primary Continue/Try again (or Return to active defense when a main attempt is live), Run analysis, More (Plan or replay a defense, shown only when applicable) and a quiet Leave journey last. The manual save is the header's Save, between History and Menu; a finished save shows a "Saved" toast top-right.
- **State:** Core settlement owns rewards/history. The whole graph reads actual wallet, ownership and requirements; selecting or browsing buys nothing. Parallel completions retain their separate orchestration.
- **Layout:** The compact outcome/actions lead one connected node canvas and selected purchase details. There are no separate personal body/skill/fortification purchase tabs.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.

This screen uses the same global LASTLIGHT header as the camp (wallet, Menu, History, Settings) with one page heading and no second wallet. Failed-round rest/train wording is shown once. The named continuation action reflects retry/next defense/country choice.
