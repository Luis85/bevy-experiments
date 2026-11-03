---
type: View
id: settings-controls
title: "Control remapping"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::remap_settings"
---

# Control remapping

Rebind essential actions to keys or mouse buttons.

- **Entry, exit and actions:** Select an action, then press a key/button; accepted bindings replace the armed prompt; save through settings return.
- **State boundary:** Binding capture/armed state are transient; duplicate bindings swap existing assignments instead of silently removing another action.
- **Responsiveness and accessibility:** Every action has a visible name and current binding; capture owns input so the binding press does not become a battle action.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

