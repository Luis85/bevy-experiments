---
type: View
id: settings-profile
title: "Settings profile controls"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::settings_ui"
---

# Settings profile controls

Restore preference defaults and display the campaign save location.

- **Entry, exit and actions:** Use Restore display, audio & control defaults to reset those preferences and cancel binding capture; save through settings return.
- **State boundary:** This reset affects display/audio/control preferences only; applied next-run gameplay balance remains campaign state. The displayed profile path describes persistence; restoring defaults does not erase campaign progress.
- **Responsiveness and accessibility:** A text path and fixed-camera/audio-cue guidance remain available in the scrollable settings page.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

