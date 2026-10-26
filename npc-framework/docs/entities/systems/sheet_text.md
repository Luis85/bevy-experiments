---
type: System
---
# `sheet_text`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/panels.rs`](../../../content/testbed_view/src/panels.rs) · **Plugin:** `TestbedViewPlugin` (`panels::plugin`)

Writes the F5 character sheet panel for the selected unit, through the rules capability's read-only view
(`Rules::new(rule_set, registry).view(sheet)`): rule set name, stats (speed shown as `basic_speed`), traits
with level and points, skills with points, level and relative label, the point totals (attributes,
secondary, traits, skills, spent, unspent, total), and with progression its level, XP towards the next level
and `SpendPolicy`. Placeholders when nothing is selected, the unit has no sheet, or no rules are installed.
Runs only while `TestbedPanels.sheet` is on.

## Schedule

- **Schedule:** `Update` (panel nodes spawned by `spawn_panels` in `Startup`)
- **Set:** `ViewSystems::Present`, next to `perception_text` and `perception_gizmos` (unordered)
- **Ordering:** `toggle_panels` (F5/F6, `ViewSystems::Input`) flips `TestbedPanels.sheet` and the node's
  `Display`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `TestbedPanels`, `Selected`, `SimIndex`, `RuleSet`, `RulesRegistry`, `ProgressionConfig`, `TestbedIds` (names); sim `CharacterSheet`, `Experience`, `SpendPolicy` |
| writes | the sheet panel `Text` (view entity) |
| messages / events | none |

## Determinism

View-only: reads the sheet, never writes it. Point spending goes through the recorded `SpendPoints` command
elsewhere, never through this panel.

## Performance

- **Mechanic tag:** `view.panels` (via `testbed_view::perf_tags` prefix `testbed_view::panels`) · **Budget:**
  none in `NPC_BUDGET_TABLE` · no span.

## Related

- [`perception_text`](perception_text.md) — F6 in the same plugin.
- [`testbed_view.update_text`](testbed_view.update_text.md) — F1.
- Components: [`CharacterSheet`](../components/CharacterSheet.md), [`Experience`](../components/Experience.md)
