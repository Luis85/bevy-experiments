---
type: View
id: return-to-menu-confirmation
title: "Return-to-menu confirmation"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/modal.rs::show_confirmation, confirmation_text, confirmation; crates/horde_game/src/ui/escape.rs::screen_action"
---

# Return-to-menu confirmation

Ask before Escape leaves the journey's top screens for the main menu.

- **Entry, exit and actions:** Escape opens it on Results, and on Camp home unless Camp home's Escape goes back to a settled Results. "Return to the main menu?" / "Your journey is saved and every field stays paused until you resume." The amber "Save and return to main menu" pauses, saves and opens the menu after the durable acknowledgment; "Stay", Escape and a backdrop click close it. The header's Menu button and Management's "Save and return to main menu" leave without this question.
- **State boundary:** `AppModal::ReturnToMenu` in the single modal slot; the buttons wait while a durable transition or a save is pending. A storage error replaces it.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
