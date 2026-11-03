---
type: View
id: settings-display
title: "Readability and effects settings"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::display_settings"
---

# Readability and effects settings

Adjust interface scale, silhouettes, numbers, flashes/movement, guidance and blood.

- **Entry, exit and actions:** Change labeled sliders/checkboxes; save through the settings return action.
- **State boundary:** Preferences affect rendering and guidance; they cannot change authoritative hit or damage outcomes.
- **Responsiveness and accessibility:** Reduce flashes and movement retains essential quiet cues. Scale supports 0.75–1.35 in the current UI.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

