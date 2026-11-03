# Lastlight UI redesign — 2 November 2026

The user requested a coherent, intuitive overhaul of the complete native UI.
The existing survival art, olive/amber palette, fixed eleven-lane battlefield,
mechanics, saves and typed command boundary remain product constraints.

## Direction contract

The interface serves the next deployment. Campaign selects a front; Briefing
explains the encounter and chooses its defender; preparation changes that
defender; deployment creates a paused battlefield; debrief leads to the next
useful action. This replaces the former flat seven-tab information hierarchy.

Camp has three destinations: Campaign, Prepare and Operations. Prepare has a
briefing and the existing equipment, character, survivor, skill and defense
pages. The planned country/defense/defender and a return to briefing persist
across those pages. The active personal field and live expeditions have direct
return/observe routes. The global clock remains explicit.

Campaign uses a bounded country browser/map and a compact actionable dossier.
Selecting a starting country leads to briefing; planning another front first
uses the existing EnterCountry command. Detailed regional facts collapse below
the primary action. Briefing collects stage, defender, loadout, boss timing,
conditions and supply before creating an attempt. No automatic purchases.

Equipment puts usable inventory and item actions before presets and exhaustive
comparison. Operations puts live fields and urgent outbreaks before recruitment
and roster management. Debrief gives victory/defeat/liberation one contextual
next step, retaining retry, loot review, recommendations, repeats and logs.

## Visual and interaction grammar

Native egui controls, compact strong headings, consistent spacing and one amber
primary action per task. Quiet management controls retain all capabilities.
Use content-driven columns on wide screens and stacked sections on compact
screens. Avoid full-width paragraph walls and decorative dashboard cards.
Essential mouse-only combat actions stay reachable. All eleven lanes remain
visible at 1440×900 and 1050×700 with 135% interface scaling.

Code-first implementation follows the user's existing preference. New raster
art and image mockups are unnecessary for this navigation/interface redesign.
No HTML/CSS detector applies to this native Bevy/egui surface.

## Acceptance

Fresh country -> briefing -> equipment -> briefing -> paused deployment ->
combat. Debrief -> an actual suggested improvement -> briefing/retry. Mara,
construction, active-field returns, outbreaks, expeditions, guards, reservations,
settings/remapping, sale confirmations, saves and all existing mechanics remain
available. Tests preserve assertions, updating intentional UI labels/routes.
Final native views and Windows interaction, independent review and a verified
portable package establish the handoff; compilation alone is insufficient.

## Verification outcome

The integrated workspace gate passed 194 tests (81 application, 113 core) and
strict Clippy. Existing behavior assertions were retained. An independent review
inspected 44 first-round native views, followed by correction of its NPC sidebar
clipping finding. All four normal/compact and collapsed/expanded confirmation
views were scored resolved. A separate behavior review confirmed the corrected
outbreak navigation preserves the requested context on a rejected front change.
The final matrix contains 46 views. Actual package/window checks and exact
evidence boundaries are maintained in [VALIDATION.md](VALIDATION.md).
