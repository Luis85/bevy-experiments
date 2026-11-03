---
type: View
id: run-history-detail
title: "Saved run detail"
parent: run-history
status: Implemented
source: "crates/horde_game/src/ui/history.rs::detail"
---

# Saved run detail

Inspect one completed run's applied context and already-banked results.

- **Actions:** Select a row in History; expand Frozen balance or Deployed equipment. No equip, award or replay transaction is attached.
- **State:** The record captures mode/outcome/country/stage/template/defender, geometry/threat, duration, kills/shots/hits/misses, source damage, average/peak DPS, coins/kill/completion XP, quality loot counts, ending HP and cause. Gear names/levels/qualities/classes and full balance settings are frozen snapshots. Worker IDs identify delegated outcomes.
- **Accessibility:** Named fields and signed HP supplement colors. Legacy unavailable measurements are identified; current inventory changes do not rewrite recorded outcomes.

Parent IDs express containment, not exclusive navigation. Implemented records source presence, not native GUI or OS validation.
