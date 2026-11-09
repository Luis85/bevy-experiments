# Island Village experience research — 2026-11-09

The next pass should make the existing simulation easier to understand and operate.
It should preserve the confirmed 3D world, survival and permanent death, and the
same recorded gameplay actions for NPCs and players. This is a research and change
proposal, not a claim that the recommendations have been implemented.

## Baseline and evidence

The orchestrator reports 369 focused tests, formatting and Clippy passing before
this research phase. The inspected native rehearsal is
`target/playtest-player-flow-6da86f62ea/summary.json` (`ok: true`). It completed a
material-backed cottage, berry planting/watering/growth/harvest, a conversation,
and Save → Title → Continue → Quit. It used enabled widget activation plus an
explicit recorded, actor-relative exploration fallback. No materials were granted
and no actor was teleported. No nearby buyer was found for a sale, so native sale
completion remains unverified. Physical input and assistive technology were not
tested by that rehearsal.

The latest local captures show two especially useful gaps:

- [Completed cottage, 1024×768](../../target/playtest-player-flow-6da86f62ea/shots/completed-cottage-1024x768.png):
  the plot selector consumes the workspace; completed-home controls are below the
  visible area. Feedback still reads `Homestead build_cottage`.
- [Growing garden, 1024×768](../../target/playtest-player-flow-6da86f62ea/shots/growing-garden-1024x768.png):
  bed state is below the plot list, and guidance still says “Start your garden”
  after the first bed has been planted and watered.
- [Neighbour help, 1280×720](../../target/playtest-player-flow-6da86f62ea/shots/neighbour-help-1280x720.png):
  attributed dialogue is present in the world, but the selected conversation is
  below a long resident list and feedback reads `Village talk`.
- [Claimed site, 1024×768](../../target/playtest-player-flow-6da86f62ea/shots/claimed-missing-materials-1024x768.png):
  the footprint is visible, but the real material requirements are below the fold.

These files are disposable local evidence, not committed visual oracles. Earlier
diagnostic cones, hero framing, redundant scroll controls, caption and save-label
issues were already addressed in the baseline polish; they are not new findings.

## Primary-source research

Accessed 2026-11-09. Each source summary is under 100 words. The proposed application
is an inference for this project, not a statement made by the source authors.

| Source | Evidence and useful lesson |
| --- | --- |
| **1. ConcernedApe, [Stardew Valley 1.6 full changelog](https://www.stardewvalley.net/stardew-valley-1-6-update-full-changelog/)** | The official changes include clearer construction duration, more persistent notifications, explicit placement failure, relevant camera framing, reactive dialogue, seasonal detail and smoother positional sound. The useful lesson is to make actions, consequences and the surrounding world legible. For this island, truthful completion feedback and visible selected-site state have more immediate value than adding another crop before the existing loop is clear. |
| **2. Shining Rock Software, [UI Redo: Controllers](https://shiningrocksoftware.com/2021-11-17-ui-redo-part-5-controllers/), 2021-11-17** | The Banished developer describes how accumulated UI complexity made late controller support expensive. A remappable input layer and focusable controls made the next system simpler, while designing for different devices encouraged common layouts. Preserve one action model and semantic widgets here; verify keyboard paths now rather than treating another input method as a second gameplay implementation later. |
| **3. Shining Rock Software, [UI Redo: Scaling](https://shiningrocksoftware.com/2021-11-05-ui-redo-part-4-scaling/), 2021-11-05** | The developer explains why fixed pixel layouts become too small on high-resolution displays and why stretching existing assets is inadequate. The intended result is comparable physical readability across resolutions. This supports explicit UI/text scaling and reflow tests; it does not require replacing Bevy's text renderer with the author's particular font technology. |
| **4. Wube, [Friday Facts 426: Resource search and assembler GUI](https://factorio.com/blog/post/fff-426), 2024-08-30** | Resource search addresses hard-to-find world objects, and the assembler separates recipe information from the specific output item so each control has a clear purpose. The project application is to separate site selection from selected-site operations and to keep known supplies, recipes and outputs distinguishable. An observation-scoped destination list is appropriate; an omniscient resource finder is not. |
| **5. Wube, [Friday Facts 412: Undo/Redo improvements](https://factorio.com/blog/post/fff-412), 2024-05-24** | Wube describes undo feedback, previews showing affected objects, context identifying the location, and restoring all relevant parts of an operation. Terrain undo should therefore be a complete, validated inverse operation with an understandable preview. A button that partly restores a brush while silently leaving some changed cells would create false confidence. |
| **6. Wube, [Friday Facts 407: Automating a soundtrack](https://factorio.com/blog/post/fff-407), 2024-04-19** | Its variable tracks use authored samples, layers, sections and transitions to provide long-session variety without replacing the composed soundtrack with unrestricted randomness. This suggests a bounded presentation-side ambience design for the island. It does not justify adding a large procedural audio framework or letting audio timing influence simulation. |
| **7. Microsoft, [XAG 101: Text display](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/101)** | The guidance covers readable default text, scalable text and layouts that retain content and function without two-dimensional scrolling. Its PC reference is 18 pixels at 1080p, with scaling up to twice the default minimum. Our current logical-pixel screenshots are not a compliance measurement; test text, recipe details and dialogue at representative DPI and larger UI scales. |
| **8. Microsoft, [XAG 112: UI navigation](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/112)** | The guidance calls for consistent navigation, predictable focus order, complete digital-input paths, and ways back from submenus. Apply this to the whole title/player/inspector/editor journey. Spectator access should not depend on creating a mortal character first, and a death screen should not permanently obstruct the colony view. |
| **9. Microsoft, [XAG 115: Errors and destructive actions](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/115)** | Errors should explain the problem and recovery where it occurs; consequential operations should be reviewable or reversible. The existing staged saves, backups and recoverable failures fit this direction. Improve action-specific failure wording and authoring feedback without adding confirmation dialogs to every ordinary gathering or one-item trade action. |
| **10. Microsoft, [XAG 103: Additional cue channels](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/103)** | Important cues should not depend solely on color or one sensory channel. Text, visual indicators and audio can reinforce one another. Preserve numeric needs and written terrain refusal reasons, add plain completion/status text, and ensure future work/harvest sounds also have visible equivalents. Optional ambience must not become necessary to understand danger or success. |

## Prioritized changes grounded in this code

P1 means an existing requested flow is obstructed. P2 means a usable flow is
misleading or unnecessarily difficult. All changes remain subject to orchestration
and bounded implementation ownership.

| Priority | Evidence and practical change | Ownership and verification |
| --- | --- | --- |
| **P1 — Complete spectator entry and death recovery** | `frontend::start_play` selects Character whenever the visitor can live, including `AwaitingArrival`. New only offers character entry. Separately, `player_hud::active` remains true for a dead actor in every mode, and the full-screen death overlay offers only successor/menu. Add a secondary **Oversee the village** entry that leaves arrival pending; add **Keep watching the village** after death. Keep successor available later. This implements the user's original inspector-first option, not a new difficulty mode. | `apps/actor_testbed/src/frontend`, `island/session.rs`, `player_hud/{mod,actions}.rs`. The host owns entry/pause/files; dismissal is view state scoped to actor and epoch. Test New as inspector has no `JoinIsland`, NPC time advances, later Play character joins once, and dead-character spectating never revives or replaces the actor. Keep Landscape tools reachable through Pause. |
| **P2 — Put the selected site's work first** | `player_hud/model.rs` emits every plot before selected details. Latest cottage/garden captures show the resulting buried requirements, completion and beds. Use a compact selected-site summary with **Change site**; show construction requirements, current build state or selected bed before the full selector. Apply the same principle to a selected neighbour's conversation. | `player_hud/{model,actions,mod}.rs`. Preserve plot IDs and actor-scoped knowledge. Test selection survives refresh/resize, Change site restores the list and focus, and selected controls are initially visible at both baseline sizes. |
| **P2 — Context must describe an actionable nearby human** | `model::context` prefers any selected target before its distance-filtered candidates. `People` and context also treat generic perception `kind == actor` as a neighbour; wildlife uses that kind too. Filter neighbours through observed `village.social.relationships`, which already contains known resident IDs, names and roles. Apply the nearby criterion to selected context targets; distant remembered destinations can still be approached through explicit workspace actions. | `player_hud/model.rs`. Add fixtures containing a nearby animal, distant selected resident and nearby human. Verify no animal talk action, no stale far-away E prompt, and no new information read from the global world. Show the already-observed role in People so a player can find a relevant merchant. |
| **P2 — Report outcomes in player language** | Latest captures show `Homestead claim_plot`, `Homestead water_crop`, `Homestead build_cottage` and `Village talk`. The adapter already has submission/refusal tracking and scoped events. Replace internal labels with clear requested/running/completed/failed wording; report a completed cottage or harvested amount from actual outcomes. Do not announce success when only a command was queued. | `player_actions.rs`, `player_hud`. Test receipt matching by actor/epoch/handle; no stale success after reset, refusal or cancellation. Capture gathering, construction completion, harvest and a failed approach. Keep debug detail available to inspectors rather than making it the primary player message. |
| **P2 — Make time and mode semantics visible** | Host speed keys and colony controls can change `SimControl.multiplier`; mode changes preserve it. Player HUD shows paused/running but no rate, while duration copy divides by 20 ticks/second. Show the current rate and label durations as simulation time or adjust the estimate. Mode changes currently call `release → stop_work`; decide and communicate whether committed work continues when leaving Character. | Host/frontend, `island_control.rs`, `player_actions.rs`, HUD. Test 1× and accelerated inspector → character round trips, paused resume, and committed construction during a mode switch. No change to the fixed authoritative tick or simulation numbers is needed. |
| **P2 — Acknowledge gardening already underway** | `player_journey::project` says “Start your garden” whenever any owned bed is empty, even with another bed growing. Use “Plant another bed” or similar when observed beds are already planted, and summarize growing/ready/needs-water beds. Keep survival precedence and the existing no-reward projection. | `village/player_journey.rs` and its integration tests. Cover an empty garden, one growing bed plus empty beds, a thirsty actor, and a ready crop. Do not persist a new quest counter. |
| **P2 — Keep discovery and commerce connected** | Craft loses its directional exploration actions once any source is known; discovering the well can still leave timber/berries unknown. The native loop needed an explicitly recorded exploration fallback. The parallel implementation audit also found that actionable targets expire with short perception memory despite existing per-actor `SeenStatics` records. Retain identified immovable places through the existing observation contract, not a new UI-only map. Keep a compact way to explore. Use known roles and explicit approach/retry feedback for moving merchants. | Content observation, `player_hud/model.rs` and shared approach adapter. Test expired perception with retained discovered station, no invented current stock or moving-person position, first well discovery with missing timber, and changed quotes. Verify one actual native buy/sell with real balance and stock deltas; the baseline did not prove this. |
| **P2 — Improve readability without changing the art direction** | The latest screenshots have a coherent Barlow/dark/gold visual identity. The remaining issue is information density, especially long selectors and small detail text. Prioritize disclosure and predictable focus before adding decoration. Add a bounded text/UI scale setting when all shell and gameplay panels can reflow together. | Shared `game_ui`, panel scrolling and host-owned presentation settings. Test 1024×768, 1280×720, representative Windows DPI, larger scale, keyboard-only tab/back/close, disabled reasons and long failure text. Screenshots alone cannot prove assistive-technology support. |

## Atmosphere and authoring scope

The native images now show the 3D cottages, work areas, crops and attributed
dialogue without the original diagnostic clutter. Favor feedback tied to actual
work and season: crop stage readability, chimney/work activity, restrained day/night
lighting and resident conversation about observed events. New dialogue should use
public world facts and the player's own relationship state, not private NPC stock
or diaries. More dialogue variety is a content opportunity; romance, festivals,
quests, interiors and a large crop catalog are broader features, not prerequisites
for correcting the current interaction loop.

The repository asset scan found no `.ogg`, `.wav`, `.mp3` or `.flac` files under
`assets`; no audible playtest was performed. A later sound pass needs authored,
licensed assets, separate ambience/effects volume, distance attenuation and event
deduplication. Keep those systems presentation-only. Do not substitute arbitrary
generated tones for a considered island soundscape merely to check an audio box.

Terraforming already has a brush preview, written refusal reasons and protected
occupied/building/route cells. A small next step is clear applied/pending/refused
feedback and persistent visibility of the selected brush. Full undo is a separate
bounded design: capture before/after cells for one committed operation, preview
the inverse, revalidate current state, and apply it atomically through recorded
commands. Do not fake undo through direct terrain writes or a partially applied
series of reversals. Persisted-history/schema work must follow repository policy.

## Acceptance for the improvement pass

1. Capture title → inspector without arrival → later character entry, and death →
   keep watching → later successor, in the same window and saved world.
2. Capture selected cottage requirements/completion and selected growing/ready
   bed with controls visible immediately at both baseline sizes.
3. Verify nearby E prompts exclude wildlife and stale distant selection, and that
   People shows only the observer's known residents and public roles.
4. Capture plain, truthful action completion/refusal, visible time rate and a mode
   transition during committed work.
5. Complete one quoted native trade; report a genuine inability to find a buyer as
   a limitation rather than a passing transaction.
6. Run affected regression tests, native recapture and the full workspace gate
   after integration. Preserve protected replay/golden files and current saves.

The web references justify design directions; the code and screenshots determine
which changes are needed here. None of this research establishes full gamepad,
screen-reader, audio, localization or accessibility compliance.
