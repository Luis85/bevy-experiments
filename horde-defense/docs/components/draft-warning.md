---
type: View
id: draft-warning
title: "Unapplied difficulty draft"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/modal.rs::show_draft_warning, DRAFT_WARNING_TITLE, LeaveSettings"
---

# Unapplied difficulty draft

Stop a Settings leave route from dropping an unapplied difficulty draft.

- **Entry, exit and actions:** Escape, the amber save-and-return primary and "Save and return to main menu" open it while the Gameplay · difficulty draft differs from the applied next-run settings. "Unapplied difficulty changes" offers the amber "Apply and return" (dispatches `SetBalance` for the draft and then leaves; disabled with "Fix the highlighted value first." while the draft does not validate), "Discard and return" and "Keep editing". Escape and a backdrop click keep editing.
- **State boundary:** `AppModal::DraftWarning` remembers which leave route it interrupted (`LeaveSettings::Back` or `Menu`). The draft is UI state; only Apply commits it, for the next run, never for an active field.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
