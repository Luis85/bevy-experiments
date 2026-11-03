---
type: View
id: briefing-progression
title: "Next-defense progression preview"
parent: briefing
status: Implemented
source: "crates/horde_game/src/ui/briefing.rs::briefing"
---

# Next-defense progression preview

Inspect what the next committed victory changes before choosing a loadout.

- **Entry, exit and actions:** Expand Next victory in Briefing; inspect the current and next rank, arrival groups, new gear level, enemy health/contact damage and rewards; collapse it or return to the planned mission.
- **State boundary:** Both columns derive from bounded progression catalog snapshots. This is a read-only forecast: it grants no rank, item, rewards or skill and does not alter an active field's frozen snapshot. A successful replay or NPC/worker completion raises threat once; defeat/withdrawal do not. Forecasts include known completion XP and exclude unknown future kill XP.
- **Responsiveness and accessibility:** Written current-to-next values explain changes independently of color and the disclosure stays in the normal scrolling briefing flow. Character MAX, lane count, population, item level and numerical threat support have separate bounds. Next-victory values use the actual core forecast APIs.

Repeating an encounter-template name does not reset campaign threat after defense five. Item acquisition level and the levels of already owned equipment remain distinct.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

The forecast uses committed balance settings and actual width/rows, with known completion XP only. Normal geometry starts at 3/12 and grows by two lanes/two yards every three character levels, capped at 17/30; edited settings apply only at deployment.
