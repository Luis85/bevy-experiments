---
type: View
id: settings-audio
title: "Audio settings"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::audio_settings"
---

# Audio settings

Set master, combat-effects and atmospheric volume.

- **Entry, exit and actions:** Adjust the three labeled sliders; save through settings return.
- **State boundary:** Volumes control presentation gain, never combat or strategic timing.
- **Responsiveness and accessibility:** Important sounds also have visual cues; sliders support quiet or muted play.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

