---
type: View
id: settings-display
title: "Readability and effects settings"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::display_settings, readability_settings, scale_slider"
---

# Readability and effects settings

Adjust the window and frame presentation, interface scale, silhouettes, numbers, flashes/movement, guidance, blood and quality markers.

- **Entry, exit and actions:** Display: "Fullscreen (borderless)", "VSync" (disabled while Low latency is on) and "Low latency" ("(applies after restart)" while its render-thread change waits for the next start); window mode and present mode apply live. Readability & effects: "Interface scale (applies on release)", which commits when the drag ends, the outline, number, reduced-effects, guidance and blood checkboxes, the Quality markers choice, "Colour-blind quality palette" and a six-quality preview. Save through the settings return action.
- **State boundary:** Preferences affect rendering and guidance; they cannot change authoritative hit or damage outcomes.
- **Responsiveness and accessibility:** Reduce flashes and movement retains essential quiet cues. Scale supports 0.75–1.35 in the current UI.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

