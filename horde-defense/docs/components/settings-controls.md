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
- **State boundary:** Binding capture/armed state are transient; duplicate bindings swap existing assignments across both tables instead of silently removing another action, and an extra binding that equals a core one loads as unbound. Escape belongs to Pause alone, because the Escape router consumes it on every screen: rebinding Pause away from Escape leaves an action that held the new key Unbound rather than handing it Escape, and loading a settings file unbinds any action other than Pause that holds Escape. The Controls hint says so ("Duplicate bindings swap; an action that would take Escape is left unbound."). A core action left without a key (Pause took it, or an old file's Escape) is never shown as an empty key in battle: the key legend, the Field actions shortcuts and the Emergency aid and Brace hovers read "Unbound", and the first-defense lesson and the gun's hover say "<action> is unbound" and name Settings › Controls. `bindings` stays eight strings in `settings.json`; the extra actions are their own field.
- **Responsiveness and accessibility:** Every action has a visible name and current binding; capture owns input so the binding press does not become a battle action.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

