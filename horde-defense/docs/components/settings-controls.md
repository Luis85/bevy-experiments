---
type: View
id: settings-controls
title: "Control remapping"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::remap_settings"
---

# Control remapping

Rebind the eight core actions and the two extra actions to keys or mouse buttons.

- **Entry, exit and actions:** The eight core rows ("Pause (battle)" among them), then "World clock (camp home)" (default P) and "Cycle attack" (default Q). Select an action, then press a key/button; accepted bindings replace the armed prompt, and Escape cancels the prompt instead of being captured. Save through settings return.
- **State boundary:** Binding capture/armed state are transient; duplicate bindings swap existing assignments across both tables instead of silently removing another action, and an extra binding that equals a core one loads as unbound. `bindings` stays eight strings in `settings.json`; the extra actions are their own field.
- **Responsiveness and accessibility:** Every action has a visible name and current binding; capture owns input so the binding press does not become a battle action.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

