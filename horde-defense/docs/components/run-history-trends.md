---
type: View
id: run-history-trends
title: "Filtered run trends"
parent: run-history
status: Implemented
source: "crates/horde_game/src/ui/history.rs::trend"
---

# Filtered run trends

Compare actual average DPS, banked coins and deployment threat across recent matching runs.

- **Actions:** History filters choose the series; hover a point for run ID and value.
- **State:** Up to forty matching saved records are plotted oldest-to-newest. Simulation seconds define DPS even at 4× playback. Plotting does not delete older data, normalize away difficulty context or rerun combat.
- **Layout:** Three named metric plots share columns at 660 available units and stack below. Labels, values and hover details accompany red/amber/green series; chart color is not an outcome verdict.

Parent IDs express containment, not exclusive navigation. Implemented records source presence, not native GUI or OS validation.
