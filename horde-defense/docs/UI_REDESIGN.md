# Lastlight UI redesign — 2 November 2026

The user requested a coherent, intuitive overhaul of the complete native UI.
The existing survival art, olive/amber palette, fixed eleven-lane battlefield,
mechanics, saves and typed command boundary remain product constraints.

## Direction contract

The interface serves the next deployment. Campaign selects a front; Briefing
explains the encounter and chooses its defender; preparation changes that
defender; deployment creates a paused battlefield; debrief leads to the next
useful action. This replaces the former flat seven-tab information hierarchy.

Camp has one navigation row (revised 4 November and 14 November, below): Journey upgrades,
Briefing, Equipment, Character, Mara, World map and Operations. The planned
country/defense/defender and a "Go to briefing" footer persist across the
Prepare pages. The active personal field and live expeditions have direct
return/observe routes. The global clock remains explicit.

Campaign uses a bounded country browser/map and a compact actionable dossier.
Selecting a starting country leads to briefing; planning another front first
uses SelectCountry, which never deploys (W3-18). Detailed regional facts collapse below
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
  resumes a waiting defense. (Superseded on 14 November 2026: one router decides
  every Escape, and the world clock has its own key; see "Escape and decision
  18" below.)
- **Smaller fixes.**
  - History's "Clear filters" is as tall as the dropdowns and idle without a
    filter.
  - The outbreak button says "Defend now", because it deploys.
  - The world map no longer advertises G/D keys.
  - The header says "N liberated".
  - Companion nodes draw a two-survivor pictogram. Blueprints and masteries
    draw trap, wall and turret pictograms.

Before/after: `docs/screenshots/foundation/before-*.png` and `after-*.png` (plus `after-skills-search.png`).

## Production pass — 14 November 2026

Plan, findings, loop map and outcome: [plans/2026-11-05-production-pass.md](plans/2026-11-05-production-pass.md); the ux-flow design is [plans/production-pass/design-ux-flow.md](plans/production-pass/design-ux-flow.md), whose integration rulings win over its spec text. The as-built loop (every screen's amber primary and Escape target) is [JOURNEYS.md](JOURNEYS.md), "Loop map", and every new View is listed in [VIEW_HIERARCHY.md](VIEW_HIERARCHY.md), "Production pass views". This section supersedes the earlier sections where they disagree: the field is no longer a fixed eleven-lane clearing, Results is no longer the tree's page, and Escape no longer drives the world clock.

### Escape and decision 18

One pure function, `ui::escape::escape_action`, decides what an Escape press means, and `escape_router` applies it before any screen reads the keyboard; nothing else reads the key (`escape_is_read_only_by_the_router`). The topmost overlay answers first, in this order: a rebind prompt, a focused text field (left to egui), an application modal, the new-journey confirmation, the operation details window, an open popup (left to egui), the Operations observer window and an armory sale confirmation. With no overlay the screen answers: the menu asks to quit, survivor selection and the world map before the first start go to the menu, a camp tab goes to Camp home, Camp home goes to a settled Results or asks to return to the menu, Results asks to return to the menu, Upgrades goes to Results (or Camp home), History closes, and Settings saves and returns unless the difficulty draft is unapplied.

In battle Escape cancels a retreat hold, else closes construction without pausing, else passes the press to the Pause binding only when Pause is bound to Escape (decision 18, literally: the default bindings do bind it); with Pause on another key Escape does nothing in battle. Every action but the ones left to egui, to Pause or ignored consumes the key, and `keyboard_owner` keeps the battle and camp bindings silent while a modal, a rebind prompt or the details window owns the keyboard. While a save, a reset or another durable transition is pending, Escape navigates nowhere. `matrix_table` pins every row and `properties` checks the router over every context. The foundation pass's camp Escape (the world clock) is retired: the clock has its own key, P by default, on Camp home only.

### One amber primary

`ui::primary` draws the one primary of a screen: amber fill (239, 181, 82), ink strong text, at least 190 by 40 points where the column allows, a lighter amber under the pointer, amber at 0.4 with muted text when disabled and a 1-point ink stroke when focused. `primary_in_bar` is the battle bar's paused Begin defense or Resume defense at the bar's size, and `primary_in_modal` a modal's own primary. Each records itself in a per-frame registry that `draw_screen` clears first, and the headless tests read it: `at_most_one_screen_primary_matrix` holds every screen to one, and the full loop test asserts exactly one at each of its steps. A running battle has no primary.

Labels come from core: `label()` on Slot, Policy, DamageKind and RecruitStatus, defender names from core cards (no hard-coded "Mara"), and prices from the core price functions; `no_debug_format_in_player_text` keeps `{:?}` output off the screen.

### Camp home, Results and Upgrades

Camp home (`Tab::Home`) is the camp's default tab: the next defense with the defender picker, the defender roster with core status, readiness and power, alerts with "Plan response", expeditions, offers and recent reports, and a footer with its `home_primary` and the world clock, which only Home shows. The camp row has six entries (Camp, Journey upgrades, Briefing, Equipment, Character, World map); Mara and Operations are drill-downs under Home with a "‹ Camp home" breadcrumb. Later countries are chosen with `SelectCountry` ("Plan defense", "Manage country", "Plan response"), which never deploys; the Briefing's "Deploy personally" or "Deploy <name>" dispatches `BeginDefense`. One callout at a time announces a newly unlocked feature under the camp row ("Show me", "Got it"), remembered per profile in `settings.json`.

Results and Upgrades are two screens (`Screen::Upgrades`, so `Screen::ALL` has 8). Results is the outcome strip and inline content (the loot block, the liberation recruit block, the defender picker and the affordable-upgrade count) above a bar of exactly three buttons in fixed places: Upgrade, the continuation and Leave journey; the amber one is Upgrade only after a defeat or withdrawal whose recommended node can be bought now. Upgrades is the camp row, the amber continuation (Back to battle, Choose next country or Return to active defense) beside Back to camp, and the tree at full height. Liberation leads to the Recruit modal, which ends on the amber "Back to camp".

### Modals and toasts

One modal slot, `StatusUi.modal`, holds the quit and return-to-menu confirmations that Escape opens, the storage modal (save, load and settings failures, with Retry, Open folder, Copy path and the right way on; it replaces any other modal), the unapplied-draft warning, the Recruit modal and the stash-apply confirmation. Escape and a backdrop click close the open modal. The manual save is one "Save" (header and battle Management), compound actions are "Save and quit" and "Save and return to main menu", and a save answers with a "Saved" toast.

Toasts never block and never take focus: up to three top-right under the header on every screen without a field, one in the battle's receipt slot. Info and Success last 3 s and Error 6 s on the wall clock; repeated text restarts, hover holds, reduced effects drops the fade. Expedition outcomes arrive as one toast each, acknowledged in the same frame, and wait while a main defense runs.

### Battle layout and the two-row bar

`BattleLayout` builds every battle rect once per frame from the canvas: the 124-point top band (both HUD corners, the attack selector and the receipt slot), the arena and the bar. The field is `art::fit_field`: a uniform zoom-to-fit of the geometry's reference clearing that keeps the lane:yard proportion (user decision U6, ruling R1), so a three-lane opening fills the arena width while every field of 11 lanes or more is unchanged bit for bit; a first personal defense keeps a 92-point hint band above the field. Construction is an overlay panel over the field's right side and never rescales it; an INCOMING chevron marks spawns it covers.

The bar stays within two rows at 1050x700 and 135% scale (`battle_bar_stays_within_two_rows_at_small_size`): Reload, Strike contact lane and Toggle firing moved to Management › Field actions (their keys and the legend stay), the status line became the battle toast, and Retreat & train is a 0.8 s hold with a growing fill (stepped in reduced effects) instead of a click. The boss banner shows "BOSS APPROACHING" for 2 s and "<BOSS> HAS ARRIVED" for 1.5 s of running wall time, the same at 1×, 2× and 4×; at 30% HP or less an 8-point red band frames the screen and the HP text reads "LOW" until HP is back above 35%. Both are the visual twins of their sounds, both have a static reduced form, and nothing in the battle shakes the screen.
