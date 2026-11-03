---
type: View
id: skills
title: "Unified personal progression tree"
parent: journey-upgrades
status: Implemented
source: "crates/horde_game/src/ui/progression.rs::skill_tree"
---

# Unified personal progression tree

Inspect and purchase from one connected data-driven personal tree.

- **Actions:** Pan/zoom/focus/select across the complete graph. Branch focus moves the camera to a path; it does not filter the tree into tabs. Select only inspects; Learn skill performs an eligible payment.
- **State:** Stable node IDs, global positions, prerequisites, victory/liberation gates, ownership and NodePayment come from core. Fresh personal body development, training/abilities, fortifications and feature unlocks share this tree.
- **Accessibility:** Camera/selection are transient. Costs and unavailable reasons remain beside the selection. Legacy CP eligibility is labeled separately from coins; NPC training is not silently modified.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.

Every node retains a readable truncated name and separate state/cost caption at the default/compact zoom; full names remain in hover and selected details. Prerequisite routing envelopes include both labels. The focus control names the selected branch and changes camera only.
