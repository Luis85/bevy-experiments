---
type: View
id: briefing
title: "Defense briefing"
parent: preparation
status: Implemented
source: "crates/horde_game/src/ui/briefing.rs::briefing, mission_actions"
---

# Defense briefing

Review a later defense, defender, item levels, boss timing, supply, threat and readiness.

- **Entry, exit and actions:** Select an accessible local defense and personal/NPC mode, review equipment or training and the Next victory disclosure, then deploy; return through preparation navigation.
- **State boundary:** Stage selection is intent; Game validates deployment, access, reservations and current progression. Threat rank, arrival groups, new gear level and effective boss time come from the progression snapshot; the next-victory preview is read-only.
- **Responsiveness and accessibility:** Loadout and readiness use columns above 820 UI points and stack below that width. Field rules are available as text.

**Current increment contract:** Briefing is optional preparation for subsequent attempts and later fronts. The first-country route bypasses it. Deploy and retry start live; opening construction is separate from explicit pause.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.


Committed next-run preset/dimensions, expected completion XP and current resolved pressure come from core preview helpers. Current fields retain their own snapshots; a changed settings draft is not an active-field difficulty change.
