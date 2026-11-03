---
type: View
id: new-game-confirmation
title: "Protect the current journey"
parent: main-menu
status: Implemented
source: "crates/horde_game/src/ui/menu.rs::menu; crates/horde_game/src/runner.rs::begin_new_journey"
---

# Protect the current journey

Confirm a deliberate new journey while preserving the previous full profile.

- **Actions:** Existing journey → New game opens the modal. Keep current journey cancels; Start new journey dispatches the durable reset service.
- **State:** The old full Game generation is archived before the fresh generation replaces the profile. Live state changes only after save success; archive/save failure stays visible and preserves the current journey. Display/audio/control preferences and difficulty remain.
- **Accessibility:** The consequence and both choices are named. This is distinct from leaving a journey, which retains current earned progression.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.

Reset is a single-writer epoch barrier; stale old-profile completion cannot overwrite or close the new journey. A new live Game appears only after matching durable reset success.
