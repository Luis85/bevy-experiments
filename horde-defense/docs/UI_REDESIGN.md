# Lastlight UI redesign — 2 November 2026

The user requested a coherent, intuitive overhaul of the complete native UI.
The existing survival art, olive/amber palette, fixed eleven-lane battlefield,
mechanics, saves and typed command boundary remain product constraints.

## Direction contract

The interface serves the next deployment. Campaign selects a front; Briefing
explains the encounter and chooses its defender; preparation changes that
defender; deployment creates a paused battlefield; debrief leads to the next
useful action. This replaces the former flat seven-tab information hierarchy.

Camp has one navigation row (revised 4 November, below): Journey upgrades,
Briefing, Equipment, Character, Mara, World map and Operations. The planned
country/defense/defender and a "Go to briefing" footer persist across the
Prepare pages. The active personal field and live expeditions have direct
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

## Tree and flow revision — 4 November 2026

Plan, findings and outcome: [plans/2026-11-04-tree-and-flow-overhaul.md](plans/2026-11-04-tree-and-flow-overhaul.md).

- **One nav row.** Camp and the Journey upgrades home share the LASTLIGHT
  header (wallet, Menu, History, Settings) and one row: Journey upgrades (while
  no main attempt is live), Briefing, Equipment, Character, Mara and Operations
  (when unlocked), World map. The Campaign/Prepare split, the second tab row,
  "Back to journey upgrades" and the Skills/Fortifications tabs are gone; old
  tab routes redirect to the one tree.
- **One primary per page.** Briefing deploys; other Prepare pages "Go to
  briefing"; a live main defense has one "Return to active defense". The clock
  control reads Resume/Pause world clock. Battle Management no longer repeats
  Withdraw beside "Retreat & train".
- **Skill tree.** Nodes are styled by core state, the default selection is the
  recommendation, and the selected node's path is highlighted. Long edges are
  stubs. Branch chips, search, "Path to…" goals, a minimap, keyboard navigation,
  a before→after detail panel and purchase pulse/cue were added, plus a compact
  layout at 1050×700 @ 135%.
- **Dense pages.** Equipment shows inventory above the fold at 1050×700;
  Briefing leads with loadout and readiness; Results has one outcome line;
  History has one title row. Disabled actions explain themselves on hover.
- **Type.** Body text, buttons and labels use Atkinson Hyperlegible; Pixelify
  Sans stays for titles, headings, the wordmark and combat numbers.

Before/after: `docs/screenshots/overhaul/before-*.png` and `after-*.png`.

## Foundation pass — 4 November 2026

Plan, findings, pacing table and outcome: [plans/2026-11-04-foundation-pass.md](plans/2026-11-04-foundation-pass.md).

- **Pointer.** Over the arena the system crosshair marks the aim instead of a
  painted cross, so the aim point has no presentation lag. The painted readout
  (ammo, reload, ordnance and placement rings) stays beside it. The window
  queues one frame instead of two.
- **First defense.** It opens paused. A one-line lesson sits over the field and
  names the player's own keys, with "Hide hints". The key legend is always
  painted, and a READY/PAUSED banner names a frozen field.
- **Results.** A loss shows its cause and the core tip in full. A victory names
  what it opened: new fronts after a liberation, and newly gated nodes. The
  outcome line truncates instead of wrapping. "Run history" moved to the header
  only. Outcomes have one name on every screen: Victory, Defeat, Retreated.
- **Escape.** Escape cancels a key rebind and closes the new-journey prompt. In
  camp it drives the world clock only where the clock is shown, and it never
  resumes a waiting defense.
- **Smaller fixes.**
  - History's "Clear filters" is as tall as the dropdowns and idle without a
    filter.
  - The outbreak button says "Defend now", because it deploys.
  - The world map no longer advertises G/D keys.
  - The header says "N liberated".
  - Companion nodes draw a two-survivor pictogram. Blueprints and masteries
    draw trap, wall and turret pictograms.

Before/after: `docs/screenshots/foundation/before-*.png` and `after-*.png` (plus `after-skills-search.png`).
