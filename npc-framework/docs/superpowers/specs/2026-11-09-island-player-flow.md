# Island Village: coherent player flow

## Confirmed brief

Retain the 3D island and survival/permanent-death model. Build an end-to-end player
experience inspired by life/farming games: title → New/Continue → arrive → gather →
craft → claim/build → plant/water/harvest → trade/socialise → save/pause/return.
The existing native visual world is the authority; the increment changes interaction
and composition rather than adopting pixel art or an unrelated interface identity.

## Experience contract

- **Title:** paused real island backdrop, clear New/Continue, readable saved-world
  identity and a straightforward new-character setup. No save is created or replaced
  simply by opening the title. Oversee the village starts or continues in Inspector,
  retaining a pending visitor's arrival until explicit character entry.
- **Entry:** one process and one window. Arrival takes explicit recorded control;
  UI explains survival and the first useful action without requiring function-key
  knowledge or CLI commands.
- **Play:** compact day/needs/currency HUD, coherent toolbar and one contextual
  workspace. Pouch, crafting, building, garden, trade, people and exploration share typography,
  control states, target selection and action feedback.
- **Discovery:** keep short directional walks and known destinations available after
  first discovery. Retain learned fixed places through existing per-actor records,
  without revealing unseen stock, foreign plot state or moving-person positions.
- **Building:** inspect a legal site and its ghost footprint, see exact stock and
  missing materials, walk into reach and apply shared claim/build actions. Cancel
  releases real reservations. The selected site's work appears before its selector.
  No remote grants or hidden teleport.
- **Farming:** grain keeps its existing contract; berry beds consume real berries
  and produce real food/trade goods. Crop definitions are explicit data.
- **Guidance:** pure projections of current inventory, known targets and estate
  state. Survival needs override introductory suggestions. No renderer-owned quest
  truth, fabricated achievements or omniscient shortcuts.
- **Pause/return:** host owns pause sources, save, new world and title transition.
  Failed saves leave a recoverable session. Existing saves and registry identities
  stay compatible.
- **Inspection and death:** mode changes retain committed work until native authority
  can detach normally. Explicit Stop retains cancellation semantics. After permanent
  death, keep watching the colony or explicitly begin a successor later.
- **Feedback:** distinguish requests from actual outcomes, show the current simulation
  rate, label durations as game time, acknowledge already planted/growing beds, and
  expose the actor's own home heat/fuel rule and known residents' roles.

## Visual and interaction direction

Preserve KayKit 3D scenery, Barlow typography, the island's near-black panels, ivory
text and gold focus/accent. Use restrained green for the primary action. The shared
`game_ui` layer supplies text/control tokens and states; feature modules own behavior.
Normal play gives the world most of the viewport. Workspaces open intentionally and
expose requirements before actions. Title/new/pause overlays own focus and block all
underlying player and developer input while active.

WASD moves, E provides contextual interaction, and the toolbar names its keyboard
shortcuts. Inspector/editor retain their established controls. Escape closes an open
workspace first, then pauses through a host request. UI actions are never direct
simulation mutations.

## Workstreams and verification

The flow agent owns application composition/session lifecycle. The player-experience
agent owns HUD, context, workspace and preview. The life-gameplay agent owns crop
definitions, shared actions and pure guidance. The orchestrator owns shared design
contracts, integration, verification and the final independent review/polish pass.

Check preserved grain and old-world behavior, berry use/harvest/trade, pure guidance,
shared input ownership, paused-menu isolation, save failure, New/Continue/return,
and the complete material-backed first-home loop. Capture title, setup, player HUD,
inventory, build preview, growth, trade and pause at both window sizes. Run the full
`cargo xtask verify --pretty` gate and report its JSON verdict and exclusions.

## Requested research and final improvement pass

After the playable increment is verified, run a subagent-led web research review
covering the player journey and UI, life-game mechanics and economy, NPC planning
and social simulation, deterministic simulation, architecture, persistence,
performance, and implementation/testing practices. Prefer original developer
postmortems, official engine documentation, and published research. Compare each
useful finding to the actual code; record source links and concrete gaps rather
than treating a reference architecture as a replacement specification.

The orchestrator selects and integrates applicable improvements, delegates the
implementation and independent review, then repeats affected checks and native
captures before the final full workspace gate. Research must preserve the user's
confirmed 3D/survival direction, established ownership boundaries, and protected
replay/snapshot baselines.

The three briefs are now recorded as
[experience](../../research/2026-11-09-experience-research.md),
[simulation](../../research/2026-11-09-simulation-research.md), and
[implementation](../../research/2026-11-09-implementation-research.md).
The selected refinement is the contracts above plus shared social availability,
pure planner outcome reasons, a bounded replenishing-shortage recovery fixture,
and visibility gates for unused observation work. Keep source proposals, authored
implementation and passing verification as separate evidence. The orchestrator's
[validation note](../../_notes/2026-11-09-island-player-flow-validation.md) records
the actual test/native outcomes.

This increment does not establish commercial Stardew Valley feature parity,
general behavior-graph authoring, specialist replacement/demographics, terrain
undo, an audio soundscape, physical-input validation or accessibility compliance.
