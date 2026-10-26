---
type: System
---
# `update_panel` (journal panel, F3)

**Crate:** `npc_view_helpers` · **File:** [`crates/npc_view_helpers/src/journal_panel.rs`](../../../crates/npc_view_helpers/src/journal_panel.rs) · **Plugin:** `NpcViewHelpersPlugin` (`journal_panel::plugin`)

Fills the journal panel: the last `JOURNAL_ROWS` (30) `Journal` entries, filtered to the selected unit (else
all units), newest at the bottom, each row cut to 76 characters. Content `Effect` entries are described by
the `EffectDescriber` resource when the content installs one (the village does: `+3 food -> #3`), else shown
as JSON. The text is rebuilt only when the visible state changes (visibility, selected unit, journal
on/off, last entry `(tick, seq)`, entry count), cached in a `Local<Shown>`.

## Schedule

- **Schedule:** `Update` (the text node is spawned by `spawn_panel` in `Startup`, hidden)
- **Set:** `ViewSystems::Present`
- **Ordering:** none beyond the set.

## Reads / writes

| Access | Data |
|---|---|
| reads | `DebugPanels.journal`, `Selected`, `Journal` (optional; `query(journal, &JournalFilter { unit, limit: Some(30), .. })`), `EffectDescriber` (optional) |
| writes | the panel's `Text` and `Visibility` (view entities only) |
| messages / events | none |

## Determinism

View-only: reads the journal, never writes it or any sim state.

## Performance

- **Mechanic tag:** `view.journal_panel` (via `default_npc` prefix `npc_view_helpers::journal_panel`) ·
  **Budget:** none · no span. Cheap when nothing changed (one struct compare).

## Related

- [`npc_view_helpers.perf_panel.update_panel`](npc_view_helpers.perf_panel.update_panel.md) — F4.
- [`click_select`](click_select.md) — sets the unit filter.
- The F3 key is bound by the view crates (`toggle_panels` in `village_view`, `toggle_debug_panels` in
  `testbed_view`), which flip `DebugPanels.journal`.
