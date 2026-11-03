---
type: View
id: run-history
title: "Saved run analysis"
parent: game
status: Implemented
source: "crates/horde_game/src/ui/history.rs::show, matching, run_list"
---

# Saved run analysis

Browse every recorded outcome using mode, outcome and front/country filters.

- **Routes:** Menu/camp History and results Run analysis enter this screen. Fields pause while open; Back/Escape restores the recorded return screen and prior pause.
- **State:** Parent-owned saved RunRecord data is read-only. Filter/selection state belongs to HistoryState; selecting never pays, repeats or resimulates a run. Older saves begin with no invented past records.
- **Layout:** The virtualized newest-first list and selected detail split at 760 available units and stack below it. Empty ledgers and empty filters explain their state. The full ledger is not truncated by the chart's last-forty display window.

Parent IDs express containment, not exclusive navigation. Implemented records source presence, not native GUI or OS validation.
