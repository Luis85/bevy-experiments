---
type: View
id: storage-error
title: "Save, load and settings errors"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/modal.rs::show_storage, open_storage, startup_modal, close_modal; crates/horde_game/src/runner/folder.rs::FolderOpener"
---

# Save, load and settings errors

Keep a failed save, an unreadable save or unreadable settings in front of the player with the ways out.

- **Entry, exit and actions:** A failed save of any intent (Autosave included) opens "Save failed": "Your progress is still in memory. Retry before closing the game.", the amber "Retry" (an Autosave retries as a manual save), "Open folder", "Copy path" and "Dismiss". An unreadable save at startup opens "Save could not be read" with "Retry load", "Open folder", "Copy path" and "Continue with a fresh journey"; "Retry load" is enabled only while nothing has changed or been saved since the load, and otherwise says why. Unreadable settings open "Settings could not be read" ("defaults are in use", the original kept as `settings.unreadable.json`) with "Open folder" and the amber "Use defaults". Every variant shows the save folder as text. Escape and Dismiss close it.
- **State boundary:** `AppModal::Storage` takes the single modal slot from whatever is open; the preempted modal is dropped, not queued. A dismissed save failure is remembered, so the same autosave error does not reopen the modal on every retry until a save succeeds. A load failure and a settings warning of the same startup share the slot: the settings warning follows once the load failure closes. "Open folder" goes through the runner (`AppCommand::OpenSaveFolder` and its `FolderOpener`); "Copy path" uses the clipboard and notes "Path copied.".
- **Responsiveness and accessibility:** The error text is written in full; no colour-only state.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
