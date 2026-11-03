---
type: View
id: character
title: "Personal stats and equipped appearance"
parent: preparation
status: Implemented
source: "crates/horde_game/src/ui/character.rs::character; crates/horde_game/src/visual_assets/player_preview.rs::defender_preview"
---

# Personal stats and equipped appearance

Inspect personal level, actual developed ranks, field stats and equipped component appearance.

The page is read-only apart from navigation to the unified tree or Equipment. Fresh XP develops level/geometry without personal CP; legacy points and their payment cap are separately identified. Actual ranks take the maximum of retained allocation floors and learned graph ranks. No independent body purchase widgets remain here. The immutable equipped profile supplies the shared renderer preview; that drawing helper alone is not another navigation View.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.

The shared preview resolves EquipmentVisual through the production player_model appearance/pose facade and shows an explicit unavailable state on resolution errors. It grants no item and has no legacy generic-body fallback. Original-bind parity and current equipped-pose acceptance are separate.
