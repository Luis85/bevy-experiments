# Player experience review and implementation — 0.21

Owner: player-experience specialist. Baseline: supplied 0.20.0 source archive.
Assessment A was completed independently before implementation, without reading
the other specialists' findings. This record distinguishes source evidence from
executed checks and native visual/device acceptance.

## Review scope

Read PRODUCT.md, DESIGN.md, the 0.20 implementation/gap/current-review/validation
records, and the game-flow, controls, guidance and playtesting contracts. Inspected
the native launcher, session flow, input/accessibility and in-world interface,
with relevant control, market and driving handoffs. Historical screenshots and
receipts were supplied evidence; they were not treated as fresh qualification.

The Impeccable context/detector was previously attempted by the integrator and
unavailable. PRODUCT.md/DESIGN.md were read directly. The skill's craft-floor and
polish references were read before editing. No saved critique snapshot was used.
The incumbent forest palette, typeface, layout, controller grid and world rules
remain the design authority.

## Findings and disposition

| Finding | Evidence in baseline | Decision |
|---|---|---|
| UX01, P1: loading recovery controls excluded from controller context | `GameFlow::controller_menu_active` admitted only Paused, while `session_buttons` exposed loading Cancel/Save-and-return/Close-failed-world actions. `accessibility::navigate` returned before activation outside that context. | Implemented in WP01: Loading is an actionable menu context; Saving stays noninteractive and world input stays blocked. |
| UX02, P1/P2: text keyboard lacked a native focus boundary and parent activation guards | `keyboard_view` had no modal tab group; keyboard buttons had no tab indices. Launcher/session/world Activate observers could accept parent events while a draft was open. | Implemented in WP02 with native event regressions. The baseline missing boundary is confirmed from source; a complete physical native input sequence is a separate qualification. |
| UX03, P2: direct-play JSON incorrectly promised clean-exit saving | Session status promised clean-exit saving for all nonregional worlds; `save_managed_json_on_exit` deliberately excludes worlds without a managed profile. The World drawer did not distinguish managed JSON. | Implemented in WP03 without changing any save behavior. |
| UX04, P2: initial RTS Activities view omits persistent actor/needs context | Activities is the default page; selected-actor/needs controls are on Command; RTS footer shows region/population information. PRODUCT.md requires immediate actor and needs understanding. | Deferred product improvement. Requires explicit selected/active/nonowned/multiselection/driver contexts plus minimum-window/150% visual acceptance before adding layout. |
| UX05, P2: long-path wrapping differed across native surfaces | Launcher labels use word-or-character wrapping; session and controller-keyboard labels did not. | Narrow wrapping applied within WP02. Actual pixel containment is not established by this structural change alone. |

The reviewed-restore target/review-identity protections already present in 0.20
remain intact. No new consent bypass was identified in the source review.
Garage unavailable-state explanations, uncoached first/returning play, physical
devices, assistive semantics, motion comfort and localization remain wider work.

## Implemented changes

### WP01: controller access during loading

`GameFlow::controller_menu_active` admits Loading and Paused. Existing
`blocks_input`, the regional failure classifier, actual session actions, and the
save acknowledgement flow remain authoritative. No saving, cancellation or
terminal failure policy was changed.

The regression traverses and activates the real native session button using the
controller input adapter in network, local and terminal-loading contexts. Network
cancel reaches the normal menu-return exit; local leave queues the normal Save
and enters Saving. The terminal button reaches the actual terminal-action policy
and returns Close with menu handoff selected. This fixture supplies the observed
terminal classification; it does not inject or certify a physical storage-worker
failure. Existing worker/terminal-classification regressions cover that boundary.

### WP02: private draft input and readable text

The text keyboard has a modal native tab group above the parent overlay. Its
draft preview and five existing editing actions provide a compact tab order;
the 50-character grid retains its current pointer/controller behavior. Native
Tab/Shift+Tab stays within the group. Semantic focus is retained when text or
selection causes the existing keyboard view to rebuild. Focus uses Terra's
existing accent outline.

Enter on the draft applies it. Enter/Space on a focused editing action activates
that action once; Space does not also enter a draft character. Apply and Discard
restore the original native field's focus. Native launcher, session and world
button observers reject parent activation while a draft is open and during its
closing frame, including a stale focused event already addressed to a parent.
Concurrent-field changes, bounded text, command-queue capacity and ordinary
controller editing checks remain in place.

Session and keyboard labels use the launcher's existing word-or-character
wrapping convention for unbroken paths/errors. No new panel, art, motion, font,
gameplay ledger or saved field was added.

### WP03: truthful persistence explanation

One small read-only helper distinguishes server-owned persistence, regional
checkpoints, managed portable saves and direct-play manual JSON. Both the
session menu and World drawer use it where they describe that contract.
Managed JSON states clean-exit saving and no periodic autosave. Direct-play JSON
names F5 or the explicit Save-and-quit/return controls and says that closing the
window does not save. Regional cadence/generation state and host ownership stay
explicit. This is presentation only; save/restore behavior is unchanged.

### WP05 integration correction found during cross-review

The world implementation now admits the actual clipped/wrapped neighborhood at
the poles. Independent cross-review found that native Loading still used the
older rectangular tile count, so the domain could initialize a six-tile polar
world that the native flow would never release. The integrator assigned the
narrow native correction to this owner.

Loading admission and progress now use the shared `living::startup_neighborhood`
helper around the active player's position, with `runtime.focus` as the fallback
when no player exists. They preserve the existing nine-page startup cap and count
only neighborhood keys present in resident source data. Render readiness requires
the same resident keys to appear in `RenderTelemetry::integrated_chunks`, which
the world owner populates only for current source identities actually integrated
by the renderer. Old visible fallback geometry and unrelated loaded pages cannot
satisfy the gate. An empty/invalid neighborhood cannot enter play. Account,
network connection/actor, pending checkpoint and storage-blocker conditions are
preserved.

The screenshot smoke-readiness segment uses the same neighborhood helper and
requires exact resident/current-integrated membership. It retains its existing
full displayed-neighborhood threshold and idle-work requirements; it does not
adopt the smaller population/loading startup cap.

## Files owned

- `crates/terra-demo/src/flow.rs`
- `crates/terra-demo/src/accessibility.rs`
- `crates/terra-demo/src/interface.rs`
- `crates/terra-demo/src/launcher/ui.rs`
- `crates/terra-demo/src/interaction.rs` (only smoke neighborhood readiness;
  reliability separately owns its metadata output-path guards)

Reliability's first-session witness proposal requires no shared flow hook, as
coordinated with its owner. Version, manifests, current product documents and
release integration remain with the integrator.

## Verification record

Source edits formatted with the pinned toolchain's rustfmt, edition 2024.
An early shared compile caught an in-progress 17-parameter system signature;
the added focus query was folded into the existing button query before the
implementation handoff. The final signature preserves Bevy's parameter limit.

New behavioral tests included in the coordinated `terra-demo` run:

- `loading_controller_reaches_cancel_save_and_terminal_recovery_without_world_input`
- `native_keyboard_tab_boundary_blocks_parent_events_and_applies_only_the_draft`
- `session_button_rejects_native_activation_during_text_entry_and_its_closing_frame`
- `launcher_button_rejects_native_activation_during_text_entry_and_its_closing_frame`
- `save_contract_copy_matches_all_four_session_modes`
- `native_loading_uses_real_polar_ordinary_and_single_tile_neighborhoods`
- `native_loading_rejects_unrelated_or_unrendered_pages_despite_total_counts`
- `native_loading_keeps_account_network_and_nonempty_neighborhood_gates`

The native-keyboard fixture uses Bevy's real InputPlugin, InputDispatchPlugin,
TabNavigationPlugin and ButtonPlugin. It supplies visibility for the headless UI
tree because it does not install a renderer. It verifies event routing, real
editable-text commit, cancellation, same-frame isolation and restored focus;
it does not establish pixel clipping or physical input-device behavior. The new
Loading tests call the actual status and transition systems, covering north/south
six-tile, ordinary nine-tile and radius-zero cases; source/render membership;
active-player versus camera focus; invalid neighborhoods; and account/network
gates. They do not inject the private regional worker state.

The integrator's first combined demo candidate compiled and ran 195 tests: 192
passed, with three new post-editor activation positive controls failing. Inspection
of Bevy's `ButtonPlugin::button_on_key_event` showed that direct synthetic
`World::trigger(FocusedInput)` fixtures had not flushed the `Commands`-queued
`Activate` before asserting. Both negative and positive assertions now flush
those events before changing guard state or inspecting queues. The real
dispatched-key Apply/Discard path already ran via `app.update`. This correction
changes the fixtures, not the production guard, and keeps all positive controls.
The subsequent coordinated run in
`docs/verification/v0.21-preflight/demo-confirmation.log` reports **203 passed,
0 failed**. This reviewer checked that all eight behavioral tests listed above
appear with `ok`, including the restored-activation positive controls and the
native WP05 admission/status cases. This run preceded the final small Space-key
width and launcher-copy correction described below. The rebuilt native candidate
completed successfully in `native-confirmation-build.log`; the integrator owns
the final release-gate results.

Cargo execution and native capture were coordinated by the integrator. The
current virtual-display preflight and captures succeeded; the earlier release's
socket restriction is historical, not a claimed present blocker. Executed checks
and the bounded visual conclusions below do not establish physical-device or
assistive-technology qualification.

## Documentation completion

The integrator assigned a final narrow documentation pass after source handoff.
`docs/GAME_FLOW.md` and `docs/CONTROLS.md` now describe the implemented 0.21
Loading recovery actions, private modal text-keyboard focus and editing controls,
closing-frame parent-action protection, word-or-character wrapping, and the four
regional/managed-JSON/manual-JSON/host-owned save contracts. Existing versioned
history and document structure were retained. The game-flow verification section
also distinguishes full displayed-neighborhood screenshot readiness from the
smaller capped gameplay admission rule. Claims were checked against the current
flow, accessibility, launcher, input and screenshot-readiness source. No code,
Cargo run, current review/gaps or validation records changed in this pass.

## Native confirmation review — 2026-10-29

The initial 1100×700 capture at 150% text showed the Space key wrapping as
`Spa` / `ce`, making the final character row taller. The authorized final polish
batch changed only that key's flex weight from 1.0 to 1.5; font sizes, configured
text scale, character selection and action semantics stayed unchanged. The same
batch replaced the launch review's direct-restore wording with the existing
shared save-contract helper. These were styling/copy changes, with no new
behavioral test or independent Cargo run by this owner.

The following fresh native captures were inspected after the code freeze:

| Capture in `docs/verification/v0.21-native/` | Observed result and scope |
|---|---|
| `confirmation-keyboard-1100-150.png` | Space is on one line. The draft preview, all five character rows, Delete/Clear/Capitals/Discard/Apply and the complete three-line instruction block are visible within the modal and window. The existing enlarged text and focus outline remain visible. No further blocker was found in this captured layout. |
| `managed-paused-1100-150.png` | The paused menu fits the window, with initial Resume focus and a scrollable action region; the final actions are below the initial visible region. |
| `managed-save-quit-focused-1100-150.png` | The same menu is scrolled to show Save and quit fully with its accent focus outline. Save and return to main menu and the wrapped restore action are also visible. The integrator reports native Tab navigation between these endpoints; this reviewer inspected the resulting images rather than independently driving the window. |
| `confirmation-review-copy-1100-150.png` | Ready to begin and its fixed Back/Create footer are visible at 150%. The save-contract paragraph is below the current scroll position, so this image does not visually confirm its exact F9 wording. That wording is supported by source/helper inspection and the earlier helper regression, not this viewport. |

The earlier `confirmation-managed-copy-1100-150.png` shows World settings with
Review world focused, not the Ready to begin screen; it is not evidence of the
save-contract paragraph. Capture receipts for the keyboard and paused-menu
images record 1100×700 dimensions and stable window PIDs before/after capture.
The confirmation launch record also identifies its binary/source inputs.

The native logs identify Vulkan **llvmpipe**, a CPU/software renderer in the
virtual display. These results establish the observed layout and logical native
keyboard/focus behavior in that environment. They do not certify physical
controller or wheel behavior, screen-reader semantics, OS-wide assistive input,
GPU performance, high-DPI behavior on other displays, all window sizes, or
uncoached-player comprehension. No further source change was requested by this
confirmation review; the code remains frozen.
