---
type: View
id: game
title: "Lastlight game"
parent: null
status: Implemented
source: "crates/horde_game/src/ui.rs::draw"
---

# Lastlight game

Application shell for the campaign, personal defense and delegated operations.

- **Entry, exit and actions:** Launch opens main-menu; fresh profiles choose survivor-selection before the first country. Screen routing (`Screen`: Menu, SurvivorSelection, Camp, Battle, Results, Upgrades, History, Settings) chooses the rest; the camp's tabs are Camp home, the Prepare pages, the world map and the Mara and Operations drill-downs. One Escape router (`ui/escape.rs`) decides what Escape means on every screen, and application modals and toasts draw over whatever screen is shown.
- **State boundary:** Game and Runtime own campaign and attempt state. The UI owns the `Screen`/`Tab` states, `Navigation` and per-screen UI resources (transient presentation).
- **Responsiveness and accessibility:** Preserve readable controls at the supported minimum window and interface scales. Important sound cues also have visible feedback.

**Current increment contract:** Confirm a personal survivor before the first country. Start journey on that country deploys the first personal defense paused, waiting for Begin defense; later countries are only chosen (`SelectCountry`) and the Briefing deploys them. Every main outcome ends on Results, whose three actions are Upgrade, the continuation and Leave journey; Upgrades and Camp home are one action away. Earned coins/items remain banked. Roughly 3–5 early runs is a pacing target; dated checks and their limits belong in VALIDATION.md. Every committed victory, including a replay or NPC/worker success, raises future threat and grants once-only completion XP; defeat and withdrawal do not. Character MAX and lane/population caps do not cap future health/damage growth.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Normal fresh deployment starts with three lanes/twelve yards and grows physically from player-level balance settings. Active and legacy geometry stays frozen. Gameplay settings commit only for the next run; saved run analysis is a read-only screen with pause/return context.
