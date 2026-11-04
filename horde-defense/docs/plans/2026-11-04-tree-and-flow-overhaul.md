# Skill tree, UI and flow overhaul (2026-11-04)

Branch `claude/horde-tree-and-flow-overhaul`, cut from local `main` at `285771b` (after the polish pass; its findings 1–11 are not redone here).

## Constraints

- Change files only under `horde-defense/`. No push, no merge into `main`.
- Rules stay in `horde_core`. Every new query (node state, path, stat preview, recommendation, search, goals) is a core view helper with tests; `horde_game` only renders. Purchases go through `Game::apply(Command::BuySkillNode)`.
- Node ids never change. Saves (v6) keep loading; no format change is planned.
- `--smoke` and the golden digests stay green. Content edits to names, descriptions and positions do not touch the simulation; any price or gate change needs a stated reason.
- Determinism work (RNG sub-streams, fixed-point) and `ContentDb` injection are out of scope.

## Baseline evidence

38 native captures (19 scenes × 1440×900 and 1050×700 @ 1.35) from `285771b`, in `target/review-before/`. A few are copied to `docs/screenshots/overhaul/before-*.png`.

## Findings (capture based)

**Skill tree** (`results`, `skills`, `skills-survival`, `skills-ordnance`, `engineering`, and the `-small` variants)

- T1. Default selection is the owned root (`1 / 169`, Survivor foundations), not the next useful node. A player starts by reading something they already have.
- T2. Affordable and unaffordable nodes look identical: both show `40 c` in white. Locked nodes all say `LOCKED`, with no reason on the canvas.
- T3. Long orthogonal wires from the root run the full width of the canvas, to Body development at x −3200 and Fortification at x +2500. They read as horizon lines rather than prerequisites. The root's amber edge to an owned child dominates the view.
- T4. Navigation is `Zoom − / Zoom + / Focus / Previous / 1 of 169 / Next`. Stepping by catalogue index jumps between unrelated branches. The wheel pans rather than zooms.
- T5. "Branch focus" is one menu button. There's no overview of the seven branches, no progress per branch, and no minimap.
- T6. No search and no goal routing. You can't ask "how do I get shotguns, Mara, fortifications or grenades?". The path to a deep node is invisible.
- T7. The detail panel says "Requires: X" but doesn't show the full chain, the total cost to reach the node, or what changes in numbers. Example: Field fitness says "+35 max health" in prose only.
- T8. At 1050×700 @ 1.35 the hub is almost unusable. Two header rows plus two toolbar rows leave about 40% of the height for the canvas, and it shows only the root.
- T9. Purchasing has no audible or visual confirmation beyond the status line (polish-pass item 5 left this open).
- T10. Body development has 100 nodes (5 attributes × 20 ranks). Each is a separate node called "Vitality 7" with near-identical boilerplate: "Train vitality to rank 7. Existing allocated ranks are preserved as a floor…". The legacy-compatibility sentence is shown to fresh profiles.

**Camp flow** (`briefing`, `armory`, `character`, `survivor`, `engineering`, `active-camp`, `world`, `campaign`, `operations`)

- F1. Two tab levels (Campaign / Prepare / Operations, then Briefing / Equipment / Your character / Mara / Skills / Fortifications) sit next to a "Back to journey upgrades" button that leads to a third hub.
- F2. `Skills` and `Fortifications` both render the same complete tree as Journey upgrades (`progression::engineering` calls `skills`). That's three routes to one screen.
- F3. The camp footer's primary action is "Back to world map" on Equipment, Character and other prepare pages. It competes with "Back to journey upgrades" in the nav bar; the two buttons go to different places.
- F4. In the active camp, "Return to active defense" appears twice (field-status strip and footer), plus "Return and resume defense" in the clock controls. That's three buttons for one route.
- F5. At 1050×700 @ 1.35, Equipment shows no inventory at all above the fold. The nav rows, page title, description, defender picker, level line, filters and quality legend fill the viewport.
- F6. In battle, "Retreat & train" sits next to "Management", which contains "Withdraw". Two labels name the same command.
- F7. On Journey upgrades, Settings appears in the page header while the camp has a global header, so the two screens have different header grammar. The Results screen has no LASTLIGHT header bar.
- F8. On Briefing, the primary "Deploy personally" is in the footer, and the field loadout's "Change equipment" button is clipped under the scroll edge at 1440×900.

## Flow map

Current (between runs):

```
Menu ─ New/Resume ─► Survivor select ─► Campaign map (starting country) ─► live Battle
Battle ─ Retreat & train / Withdraw / victory/defeat ─► Results = "Journey upgrades" (tree)
Results ─ Continue/Try again ─► Battle
        ─ Equipment ─► Camp[Prepare/Equipment] ─ "Back to journey upgrades" ─► Results
        ─ More ▸ World map / Plan defense / Character / Fortifications / Mara / Operations / Save
Camp: [Back to journey upgrades] [Campaign] [Prepare ▸ Briefing|Equipment|Character|Mara|Skills|Fortifications] [Operations]
      footer primary: "Back to world map" (Prepare pages) / "Deploy" (Briefing) / "Return to active defense"
Header: Menu · History · Settings (Camp only) ; Results has its own Settings button
```

Proposed:

```
Menu ─► Survivor select ─► Campaign map ─► Battle ─► Journey upgrades (home)
Camp nav, one row:  [Journey upgrades*] [Briefing] [Equipment] [Character] [Mara†] [World map] [Operations†]
   * when no main attempt is live; this routes to the Results home
   † shown when the feature is unlocked
   Skills and Fortifications tabs are removed. Fortifications opens Journey upgrades
   focused on the Fortification branch (the old tab routes keep working through redirects).
Footer primary per page: Briefing → Deploy ; other Prepare pages → "Go to briefing"
   (or "Return to active defense" when a field is live). Never "Back to world map".
Active defense: one "Return to active defense" (field strip). The clock button is "Resume world clock" / "Pause".
Battle: "Retreat & train" stays as the one visible withdraw; Management loses its duplicate "Withdraw".
Results gets the same global header (LASTLIGHT · Menu · History · Settings) as Camp.
```

Back behaviour: Escape keeps its pause meaning. Every Prepare page reaches the hub in one click (the first tab). The hub's "More" menu shrinks to items that aren't nav tabs: Plan or replay a defense, Save journey.

## Skill-tree redesign

### Chosen

1. **Core view helpers** (`horde_core/src/campaign/tree_view.rs`)
   - `NodeState`: one of Owned, Buyable, NeedsCoins{short}, NeedsPrerequisites{missing}, NeedsMilestone{victories, liberated}, or Unavailable{reason}. It also carries a requirement checklist with progress.
   - `path_to_node`: unowned ancestors in purchase order, plus total coins and the first blocking milestone.
   - `node_stat_preview`: typed before→after deltas from `SkillModifiers` with and without the node.
   - `recommended_node`: wraps the existing recommendation fallback.
   - `EffectCategory` tags and `search_nodes`.
   - `upgrade_goals`: weapon classes, grenades, Molotovs, fortifications, Mara, recruitment, operations.
2. **State styling** on the canvas:
   - Owned: filled amber.
   - Buyable: bright border plus an amber price.
   - Needs coins: price plus "need N" in muted red.
   - Locked by prerequisite: dimmed, with a lock mark.
   - Locked by milestone: dimmed, with "Win N" or "Free N".

   The detail panel uses the same chip.
3. **Navigation**
   - The wheel zooms around the cursor and dragging pans.
   - Arrow keys or a gamepad D-pad move to the nearest node in that direction. Enter learns the selected node; Home jumps to the recommendation.
   - "Previous / Next / 1 of 169" is replaced by a "Recommended" jump.
4. **Branch chips**: one chip per branch, showing owned/total counts, replace the cycling menu. Clicking a chip frames that branch.
5. **Minimap**: a corner overview of all 169 nodes as state-coloured dots with the viewport rectangle. Click or drag it to move the view.
6. **Search** across name, description and effect category. Matches are highlighted and the rest dimmed; Enter cycles through matches. A **Goal** menu ("Path to…") selects the goal node.
7. **Path to here**: the selected node's unowned prerequisite chain is highlighted on the canvas. Unrelated wires dim. Long cross-graph edges (longer than about 900 units) draw as short labelled stubs ("← Survivor foundations") unless they're on the highlighted path.
8. **Detail panel**:
   - State chip.
   - "What changes" table, for example "Max health 100 → 135".
   - Requirement checklist (✓/✗ with progress).
   - Path summary ("3 steps · 95 coins") with a "Select next step" button.
   - One primary Learn button with the disabled reason under it.
9. **Purchase feedback**: a short amber pulse on the node, a status line with the main delta, and a purchase cue that reuses an existing sound bank. After learning, the selection stays put; Home goes to the next recommendation.
10. **Default focus**: the recommended node. The fallback is the cheapest buyable node, not the root.
11. **Compact layout**: at a narrow width or large scale, the toolbar collapses to one row and the detail panel goes below the canvas, so the canvas keeps at least half the height.
12. **Content**: body-rank descriptions drop the legacy sentence for clarity. That sentence moves to the detail panel and is shown only for legacy profiles. Names and descriptions get a consistency pass. Prices stay as they are unless the audit finds an early-tier outlier.

### Rejected

- **Splitting the tree into per-branch pages.** The product contract is one connected graph (JOURNEYS.md), and branch chips give the same orientation without filtering.
- **Collapsing the 100 body nodes into 5 "stack" nodes.** That changes ids and ownership semantics, and needs a save migration. A detail-panel rank ladder plus stubbed long edges solves the readability problem without either.
- **A modal confirm for every purchase.** It slows the loop the hub exists for. Purchases are reversible only for legacy body points, and coins are visible on the button. A pulse plus a cue gives confirmation without a click tax.
- **Automatic graph layout** (Sugiyama-style) at runtime. Authored positions are content and stay stable for players; a runtime layout would shuffle the graph between versions.
- **Moving skill state into egui memory.** State belongs to `Campaign`; the UI keeps only view state (zoom, pan, selection).

## Waves

Each agent works in its own worktree and branch with its own `CARGO_TARGET_DIR`, and verifies with native captures. Waves merge into this branch in order.

- **Wave 1** (parallel, disjoint files)
  - A. Core tree queries and content: `horde_core` `campaign/tree_view.rs`, `skill_catalog.rs` (categories), `skills.json` text, core tests.
  - B. Camp navigation and flow: `ui/navigation.rs`, `ui/camp.rs`, `ui/context.rs`, `ui/screen.rs`, `ui/results.rs` (header and actions only), `ui/battle.rs` (Management dedupe), `ui/briefing.rs` footer, camp/results/battle tests.
- **Wave 2** (parallel, disjoint files)
  - C. Tree rendering and interaction: `ui/progression.rs`, `ui/progression/*`, purchase cue (`sfx.rs`/`audio.rs`), new review scenes in `ui/review/*`.
  - D. Armory, briefing and results surfaces: `ui/armory*`, `ui/briefing*` body, `ui/history.rs`, the `results.rs` outcome strip, and their tests.
- **Wave 3**
  - E. Shared chrome and consistency sweep: `ui/chrome.rs` plus light touches across `ui/`, covering headers, empty states, disabled reasons and spacing.
- **Wave 4**: full verify, after-captures, and docs (JOURNEYS.md, UI_REDESIGN.md, this outcome).

## Outcome

_Filled in at the end._
