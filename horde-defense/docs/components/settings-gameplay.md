---
type: View
id: settings-gameplay
title: "Next-run difficulty and geometry"
parent: settings
status: Implemented
source: "crates/horde_game/src/ui/preferences.rs::gameplay_settings"
---

# Next-run difficulty and geometry

Choose Easy, Normal, Hard or Custom for future deployments.

- **Actions:** Select a preset or edit the draft, expand Arena size & growth / Advanced combat tuning, inspect draft dimensions, then Apply to next run. Saving/returning without applying does not silently commit an unapplied draft.
- **State:** BalanceSettings validation and atomic SetBalance own the committed profile. Active main/worker balance and dimensions remain copied; the next chained run receives current shared settings. Manual slider edits mark Custom.
- **Accessibility:** Written next-run/active-field previews and validation errors explain the boundary. Scalar factors are bounded finite .25–3, geometry and supply have explicit supported ranges. The existing durable Settings return and pause behavior remains.

Parent IDs express containment, not exclusive navigation. Implemented records source presence, not native GUI or OS validation.
