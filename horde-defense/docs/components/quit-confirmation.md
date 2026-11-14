---
type: View
id: quit-confirmation
title: "Quit confirmation"
parent: main-menu
status: Implemented
source: "crates/horde_game/src/ui/modal.rs::show_confirmation, confirmation_text, confirmation; crates/horde_game/src/ui/escape.rs::screen_action"
---

# Quit confirmation

Ask before Escape closes the game from the main menu.

- **Entry, exit and actions:** Escape on the main menu opens it ("Quit Last Light?", "Your journey is saved before the game closes."). The amber "Save and quit" requests the Quit save, and the game closes after its durable acknowledgment; "Stay", Escape and a backdrop click close it. The menu's own "Save and quit" button quits without this question.
- **State boundary:** `AppModal::Quit` in the single modal slot; both buttons are disabled while a durable transition or a save is pending. A storage error replaces it.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
