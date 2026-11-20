# Player experience review and proposed work — 0.22

Status: source/document review and primary-source research completed before
implementation dispatch. The two current-run baseline screenshots below were
captured by the integrator and independently inspected by this reviewer.
Historical screenshots are not evidence for this review. No hardware,
screen-reader or uncoached-user qualification is claimed here.

## Current-run baseline flow

1. **RTS arrival / Activities — context incomplete.**
   [`01-baseline-rts.png`](../v0.22-native/01-baseline-rts.png) shows the active
   Activities recommendation, source label and wallet, but no persistent actor
   identity or needs. The footer's final line spends space on counts instead.
   The world and main action are visible; PX01 is confirmed in the actual flow.
2. **Select owned crew — group feedback incomplete.**
   [`02-baseline-crew.png`](../v0.22-native/02-baseline-crew.png) shows multiple
   selection rings, including the helicopter, while the footer says
   "Selected Explorer 1". The player cannot tell the complete selected group
   or its needs from Activities. PX04 is confirmed visually; the first-actor
   status is source-explained in `process_control_intents`.

Both captures are 1440×900 at default text scaling on the integrator's native
Linux software-rendered baseline. They establish these two visible states,
not enhanced-text quality or hardware performance. Minimum-window/150% and
remaining ownership contexts belong to the implementation acceptance matrix.

## Product goal and review coverage

The reference game should make mode, actor, needs, available action and source
provenance understandable in its first session (`PRODUCT.md`). The retained
forest-green presentation, adjustable text, ordinary native controls and
simulation-owned authority are the design constraints. Default RTS opens
Activities, while possession intentionally recedes the drawers (`DESIGN.md`,
`interface.rs::UiState::default`, `update_immersion`).

Reviewed launcher and play documentation, the retained G01–G18 register, control
selection and synchronization, inspector construction/layout, footer/needs
updates, management focus, remapping and driving-body resolution. The existing
managed recovery, explicit restore review, focus isolation, finite-economy
guidance and clear save-mode distinctions are strong foundations. This pass
should improve continuity of the player's mental model without inventing a
second quest, inventory, actor or ownership system.

## Findings

| ID | Priority | Source evidence and consequence | Proposed disposition |
|---|---|---|---|
| PX01 / inherited UX04 | P1 | `interface.rs::setup_ui` places `Label::Selected`, need labels and bars only inside `Panel::Command`. Default `Panel::Activities` lacks them; the RTS `Label::Streaming` shows regional counts. A player must change away from the recommended activity to learn whose needs matter. This contradicts the five first-session questions in PRODUCT. | Add an always-available compact selected/controlled-actor summary to the existing HUD, with detailed consistent context in Command. Preserve source and pending/failure feedback. |
| PX02 | P1 | `selected_body` falls through from an unavailable selected ID to `active_unit`. An empty selection therefore displays a specific body as if selected; a stale selection may show another body. `synchronize_control` also drops missing remote selection IDs, so receipt scope must remain explicit. | A display model must distinguish no selection, missing selection and active interaction body. Never silently claim a fallback actor is selected. |
| PX03 | P1 | `update_needs` maps any selected registered vehicle to `driving::driver_body`, which returns the current player's avatar without checking vehicle identity or possession. Inspecting another vehicle can present unrelated human needs. | Only show avatar needs as driver needs when the selected/controlled vehicle is exactly the owned account's currently possessed active vehicle. For other vehicles show vehicle context and no biological bars. |
| PX04 | P2 | Multi-selection currently uses `selected.first()` for the detailed actor/needs display and an isolated count for `Label::Mode`. A player sees one person's needs without being told that other selected bodies differ. | State group count and owned/inspect-only scope. Show explicitly labeled lowest need values across selected owned biological bodies, plus vehicle/nonbiological/missing counts. Do not average away an urgent need. |
| PX05 | P2 | Command Eat/Drink applies to the authoritative active body, while Stop/Rest affects selected owned bodies. Availability attempts to prevent mismatched consumption, but the surface does not explain the target distinction. | Add concise target explanation to Command: consumption affects the active owned body; select/activate it first; group Rest/Stop affects owned selected actors. Share the display resolution, preserve command authority. |
| PX06 | P2 | `interface::text` has a maximum width but no explicit character fallback, unlike the 0.21 session/editor text. Legal 64-byte unbroken actor/site names can exceed narrow inspector text lines. | Explicit word-or-character wrapping for in-world text. Verify long single-token names and enhanced text without reducing size. |
| PX07 | P2 | Documentation omits existing fixed X stop-orders and Ctrl+A select-crew shortcuts found in `control_shortcuts`. | Add exact shortcuts and explain which actions affect selected bodies versus the active one. |
| PX08 | Open qualification | Supported preference range is 100–150%, body text is often 11–13 logical px. The project does not establish XAG101 minimum size at the required display resolution, 200% scaling, contrast measurement, narration, or input-device equivalence. | Keep explicit G08 limitations; this patch is not XAG compliance. Broader typography/scaling/narration requires design and device acceptance across all screens, not a localized number change. |
| PX09 | Open qualification | Assisted playtest scripts are detailed, but uncoached first/returning journeys remain unevidenced. Source-derived guidance correctness is not comprehension evidence. | Preserve G03/G04 acceptance for novice/returning Supply, Home/Community and Road/Company journeys. Record confusion, dead ends and time-to-first-action without calling developer walkthroughs user research. |

## Research and application

Primary sources retrieved on 29 October 2026. These are design guidance, not an
assertion that Terra satisfies the entire guideline.

1. [Microsoft XAG 109: Objective clarity](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/109)
   recommends easy access to current goals, progress and concrete next steps.
   Apply it by keeping actor context visible alongside the existing Activities
   recommendation. Do not add a competing completion ledger; Terra's goals are
   derived from live authoritative state. Revisit returning-player task clarity
   through observation.
2. [Microsoft XAG 114: UI context](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/114)
   asks interfaces to explain where the player is, what a control does and what
   to expect. Apply it to explicit selected versus active-body terminology,
   inspect-only ownership, matching driver identity, and partial remote state.
3. [Microsoft XAG 103: Additional channels for visual and audio cues](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/103)
   advises supplementary semantic cues rather than color alone. Keep named Food,
   Water and Energy percentages, and a written low-needs indication where
   relevant. Existing bars/colors remain supporting cues. Adding written
   labels does not establish multisensory access or narration.
4. [Microsoft XAG 101: Text display](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/101)
   covers legibility, configurable sizing and avoiding lost content when text
   grows. The local implementation should wrap long labels and use the existing
   scaled layout, then inspect 1100×700 at 150%. The broader 200% and
   display-resolution guidance remains a product-wide gap.
5. [Microsoft XAG 112: UI navigation](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/112)
   covers consistent placement, predictable focus and digital navigation. Keep
   the summary read-only, reuse Command/Activities navigation, retain focused
   actions during refresh and leave all mutation in established command paths.

## Proposed implementation packages

### PX-A — accurate actor presentation model

Introduce one bounded read-only selection-summary resolver, preferably in a
small `actor_summary.rs` module with tests, called from the HUD and inspector.
Inputs are `SandboxRuntime`, `PlayerControl`, and network receipt scope. It may
read selected IDs, account active/avatar IDs, unit ownership/kind/needs/orders,
and registered vehicles. It must not mutate, enqueue, save, reseed, or synthesize
an unavailable actor. Centralize the exact driver association and displayed
need values so text and bars cannot disagree.

Expected states:

| Context | Display contract |
|---|---|
| No selection | Say no actor selected; explain existing selection/Command path. An active interaction body can be named separately, never as selected. |
| One owned person | Name, owned status and Food/Water/Energy percentages; existing order in detailed Command. |
| One non-owned actor | Name plus Inspect only. Do not imply commands/needs actions are available. |
| One parked/uncontrolled vehicle or nonbiological actor | Name, owned/inspect-only state and appropriate type; biological values unavailable. |
| Exact possessed owned vehicle | Vehicle name; explicitly labeled driver name/needs from the account avatar, only if that biological body exists and belongs to this owner. |
| Multiple selection | Count selected; distinguish owned and inspect-only/missing; minima clearly labeled as lowest among selected owned biological actors. Nonbiological bodies do not contribute zero values. |
| Missing or partial state | Name unavailable/received scope, no fallback data, no claim of whole-world completeness. |
| Pending possession | Do not call the avatar an admitted driver before authority matches active vehicle and possession. |

### PX-B — compact HUD and Command integration

Use the existing footer region, retaining the toast and controls. Replace
unnecessary idle regional census text with compact actor context, while
retaining actual loading/blocked persistence feedback. Do not increase the
world's hidden input region without updating its shared viewport contract.
Keep long names bounded in the compact text (full name in the inspector),
explicit word-or-character wrapping, and no extra actionable overlay that
could intercept world input. At small windows, ensure extra information does
not displace the main activity action off the scrollable inspector.

Command keeps detailed values and existing mutation controls. Its displayed
identity/needs use the same resolver. Consumption availability requires the
actual displayed single owned biological actor to be the active body; group
need minima must not imply group eating. Existing owned-group Rest/Stop remain.
Document the target distinction and missing keyboard shortcuts.

### PX-C — acceptance and closeout polish

Tests should prove semantics and actual wiring, not duplicate formatting:

- Empty selection and missing selected ID do not resolve to active actor.
- Non-owned actor cannot become owned via fallback or vehicle-driver lookup.
- Idle vehicle, other selected vehicle, pending possession, missing avatar and
  exact admitted driver each display the correct biological context.
- Multi-selection minima exclude vehicles and non-owned bodies; incomplete
  groups are explicitly incomplete. No aggregate masquerades as one actor.
- Remote partial receipt is named, with no whole-world crew/population claim.
- Command actions are disabled for no/foreign/mixed/missing selection where
  they cannot target the displayed body; eligible active-person action remains.
- Real label and need-bar update systems agree after selection/possession/state
  changes; selection summary works while Activities is the selected panel.
- Text fallback and shared footer/world-hit bounds remain correct at 100–150%.

Native capture matrix: RTS/Activities single owned actor, mixed crew,
vehicle-selected, empty or inspect-only, Command controls and admitted driving;
1100×700/150% plus normal reference. Root owns captures and one batched fix/
confirmation round. No claim of physical controller or screen-reader testing.

## Boundaries and dependencies

No save/wire schema, account ownership, simulation needs rates, inventory,
commands, rewards or world source policy needs to change. Restrict ownership
to `actor_summary.rs`, module declaration, interface integration/tests, and
controls/game-flow/design documentation as assigned. Coordinate `interface.rs`
with other owners. Full typography, multi-sensory output, public accounts,
new political/economic lifecycle and physical device qualification belong in
their respective tracked packages rather than this presentation closure.

## Implemented WP01–WP02

Implementation is complete in `actor_summary.rs`, `interface.rs`, the narrow
consumption/group-order branches in `interaction.rs`, and the module declaration
in `main.rs`. DESIGN, CONTROLS and the actor-context GAME_FLOW section describe
the actual semantics. The mutation-free summary handles empty, unavailable,
owned/inspect-only, group, vehicle and admitted-driver states. Named group minima
exclude non-owned and nonbiological bodies and label missing scope explicitly.
Idle regional census text gives way to actor context in the unchanged scaled
footer; loading/error feedback and input guidance remain. Full names stay in the
inspector; compact names use Unicode-safe truncation. In-world text wraps at
words or characters.

Native consumption activation captures the last displayed player, restore
revision and body ID. Dispatch rejects a changed selection, owner, active body,
pending transition or unavailable host before constructing the existing domain
command. Bare unreviewed native Eat/Drink actions cannot submit. Stop/Rest
recheck owned selection and snapshot its bounded IDs directly at dispatch,
removing an extra selection-dependent control-queue hop. Vehicle Rest is
explicitly a stopping order, not driver need recovery.

This is a native submission guard. The SDK/network Consume command retains its
existing active-body-at-authority semantics; no cross-client reservation or
wire-level reviewed-body contract is claimed. Group targets are retained at
dispatch, not at their initial button display. The domain still rejects stale
ownership/invalid group membership before mutation.

Six resolver regressions and six real UI/system regressions cover current
state, bars/text agreement, action availability, actual Activate/dispatch,
successful domain consumption, pending transition refusal, group-ID retention,
long names, unchanged scaled hit bounds and community review nonce integration.
The additional community binding correction is recorded in
[`world-cross-review.md`](world-cross-review.md).

At implementation handoff, rustfmt had successfully parsed the owned files;
Cargo execution remained with the integrator's shared compile slot. The native
candidate and final confirmation evidence are recorded below. The presence of
tests alone is not passing evidence; canonical acceptance remains a separate
release gate.

The new summary reuses the existing opaque panel/foreground tokens. A direct
linearized-sRGB calculation from `interface.rs` gives text/panel 15.77:1,
secondary-text/panel 9.87:1, accent/panel 11.66:1 and sand/panel 10.80:1. On the
secondary control surface those values are 12.22:1, 7.65:1, 9.04:1 and 8.37:1.
These calculations cover those opaque color pairs only; they do not establish
minimum rendered character size, all inactive states, screen narration or
whole-product accessibility compliance.

## Native candidate review and consolidated correction

The specialist inspected the actual candidate screenshots 33–48 in
[`v0.22-native`](../v0.22-native/). At 1100×700 and 150% UI scale, the selected
actor/group context remained readable in Activities; Command summaries stayed
within the existing scroll area, with its actions reachable in captures 43–44.
The first/third-person and admitted-vehicle captures retained identity and needs;
vehicle release showed no borrowed biological needs. Empty and inspect-only
states were explicit. This is evidence for the captured desktop states, not
physical-controller, narration or whole-product accessibility qualification.

[`48-candidate-inspect-only-attempt.png`](../v0.22-native/48-candidate-inspect-only-attempt.png)
showed Resident 20 correctly marked **Inspect only**, but the selection toast
still advertised `F2 possess`. The actual possession handler already required
ownership. The authorized correction changes only that toast in
`control.rs::selection_input`: a selection with no owned actor offers inspection
and an owned-selection next step; a mixed selection with a foreign or missing
primary offers owned movement and asks for an owned actor before possession;
possession guidance appears only when the actual primary belongs to the current
player. Ownership counts include every selected owned actor, without borrowing
the group command's bounded target count. Selection and command behavior are
unchanged.

No copy-only test was added: the existing selection event regression concerns
focus loss and does not exercise a completed hit selection. Root owns the
integrated demo run and one native confirmation of this correction. The earlier
integrated demo candidate passed 226 tests, with no failed or ignored tests, in
[`demo-confirmation.log`](../v0.22-preflight/demo-confirmation.log); that result
precedes this final toast correction and does not qualify the changed source.

## Final native confirmation — passed within the observed scope

After the consolidated correction batch, the specialist independently inspected
the following actual 1100×700, 150%-scale captures. No blocking player-experience
or review-flow defect was found in these captured states.

| Evidence | Confirmed observation |
|---|---|
| [72 — ownership-aware feedback](../v0.22-native/72-confirmation-inspect-only-feedback.png) | Resident 06 is identified as inspect-only. The selection toast asks for an owned actor before control or orders; the toast, paused control guidance and actor summary remain readable together. |
| [77 — named community](../v0.22-native/77-confirmation-community-created.png) | Confirm Guild is identified as faction #2, with the correct singular `1 member` and zero invitations. |
| [80 — default Cancel focus](../v0.22-native/80-confirmation-cancel-default-focus.png) | The complete named dissolution review is visible, including authority recheck, no transfer/refund, permanence and new-ID consequences. Cancel is visibly focused and wholly reachable. |
| [81 — keyboard cancellation](../v0.22-native/81-confirmation-keyboard-cancel-preserved.png) | The cancellation message reports no submitted command. Confirm Guild #2 and its one-member roster remain visible. |
| [86 — deliberate confirmation focus](../v0.22-native/86-confirmation-explicit-confirm-focus.png) | Explicit navigation reaches Confirm, which scrolls fully into view below Cancel. Confirmation remains available without being the review's initial focus. |
| [87 — completed dissolution](../v0.22-native/87-confirmation-dissolved.png) | The community is absent and the ordinary create/join state returns. |
| [90 — save and quit](../v0.22-native/90-confirmation-save-quit-focus.png) | The pause menu's Save and quit action is fully visible and keyboard-focused. The process receipt separately records the subsequent clean exit. |

All seven PNG hashes and byte counts were independently checked against their
capture records. Each record identifies the same native window PID **212865**
before and after capture, with a 1100×700 window. The
[confirmation process receipt](../v0.22-native/confirmation-managed-reopen-process.json)
records exit code **0**, **378 unchanged implementation inputs**, source-input
digest `ec3fb5123a82cd1d4e6f5351d8f04503d305359628a44f61d8e032d9eca5902d`, and
native binary SHA-256
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.

The independently read
[closed-world save evidence](../v0.22-native/confirmation-managed-save-evidence.json)
reports SQLite integrity `ok`, generation **60**, no remaining factions and
next domain ID **3**. Its comparison records unchanged wallet, inventory,
first-launch witness and lease-marker bytes, with an increased checkpoint
generation and no domain-ID rewind. This corroborates the captured cancellation,
deliberate dissolution and saved closeout; it does not establish crash/power-loss
behavior or general balance.

Only this review report changed during the confirmation review. Implementation
remained frozen while the integrator ran canonical acceptance. The result closes
the observed ownership-toast and community-review focus/copy corrections, within
this software-rendered Linux desktop session. Physical-controller, narration,
other operating systems, target hardware and uncoached-player qualification
remain outside this evidence.
