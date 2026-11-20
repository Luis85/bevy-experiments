# Terra World 0.24 — player experience review and implementation

> Evidence correction: the appended withdrawal supersedes the experimental
> disappearing-text assessment below. Exact RGB comparisons disprove the
> claimed screenshot difference. The experimental refresh and regression were
> removed; the final native suite contains 250 tests. Historical attempts remain
> preserved and do not establish a renderer defect or visual fix.

## Review scope

Reviewed the delivered 0.23 product/design documents, all G01–G18 requirements,
guidance/playtesting and the native Industry, Fleet, Build, community, input and
management-focus code. These source findings preceded implementation. Fresh
native screenshot evidence is owned by the integrator and recorded separately;
old screenshots are not current visual evidence.

## Prioritized findings and selected work

| Priority | Finding | Resolution selected in WP03–05 |
|---|---|---|
| P1 | `selected_site` substituted the first producer when an explicit selection disappeared. | Exact explicit selection; written unavailable state and deliberate browsing. |
| P1 | Industry resolved its facility and inverted current enabled state at dispatch. | Bind the displayed facility, actor, world and desired operation at activation; recheck before enqueueing. |
| P1 | Fleet driver/route/vehicle/stop/relief/depot selectors silently fell back. | Exact explicit selectors; recover with the existing choose-next controls. |
| P1 | Routine Fleet operations derived assignments, loads and quantities after activation. | Bound operations retain stable target/assignment/load/amount/item/direction and current panel context. |
| P1 | Build damage was unnamed and immediately selected the nearest structure at dispatch. | Named target and health consequence, explicit cancel/confirm, one-use review and safe initial focus. |
| P2 | Routine dispatch replaced an explicitly missing faction with the first membership. | Retain missing selection and explain deliberate recovery. |
| P2 | Text preference stops at 150%; Build actions have uneven readiness feedback. | Carry broader 200% reflow and remaining Build readiness into the gap register; do not increase limits without complete layout verification. |

## Primary research

- [Microsoft XAG115 — error messages and destructive actions](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/115): clear recovery and review/correction before destructive outcomes. Applied to named building health/demolition consequences and ordinary action meaning; no hold-only confirmation.
- [Microsoft XAG112 — UI navigation](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/112): consistent navigation, predictable focus, clear return paths and reflow. Applied through the existing inspector, scrolling and keyboard/controller-style focus path.
- [Microsoft XAG109 — objective clarity](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/109): reviewable current goals and useful next steps. Applied to missing-target recovery and the returning operator's named selections.
- [Microsoft XAG101 — text display](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/101): scaling without lost content/function. The existing 150% boundary is retained; this increment does not claim 200% or accessibility certification.

Primary sources were retrieved 30 October 2026. Research informs the interaction
contract; it does not substitute for representative user or device evidence.

## Implementation decisions

Native context contains the current player, restore revision and owned active
body. Process-unique checked tokens survive UI state replacement without reuse.
Reviews remain local presentation state and add no save or command schema.

Industry snapshots retain stable facility identity, ownership, recipe/source and
enabled state. Running cycle timers, stock and completion counters are deliberately
excluded. A pause/resume action therefore keeps its desired meaning without
rejecting normal production progress. Current reach and ownership are checked
again before the unchanged domain command is queued.

Fleet operational snapshots retain the displayed employee, route, vehicle,
assignment and load, or named depot plus exact item/quantity/direction. Money
transfers retain their amount. Continuous payroll, odometer, fuel and rental
countdown changes are not identity changes. Rental identity retains its garage
and security bond; dispatch eligibility checks actual expiry. Existing financial
quote/destructive-review paths remain.
The actor/world transition guard also rejects pending body changes in the same
native queue. Authority still validates its normal command rules.

Building damage begins by reviewing the named visible candidate. Confirmation
binds the full structure (including health/ownership) and exact actor/world; a
closer structure arriving afterward cannot replace it. Damage of 25 health may
destroy a low-health structure, clear homes and yields no material refund; the
review states these consequences. Tokens are one-use and Cancel makes no command.

## Fresh baseline evidence

The following complete screenshots of the pinned 0.23 native binary were opened
and inspected in this review. They show source-backed interaction issues in the
actual inspector, rather than serving as evidence for unseen queue behavior:

- `../v0.24-native/03-baseline-industry-complete.png`: named East field, out-of-reach context and the generic old “Pause / resume owned production” operation.
- `../v0.24-native/05-baseline-fleet.png`: the first-company registration state with disabled transfers before registration.
- `../v0.24-native/07-baseline-build-damage.png`: unnamed “Damage permitted structure −25” remains enabled. The integrator also recorded zero structures in this baseline state.

The incomplete `02-baseline-industry.png` was rejected by the integrator and was
not used. The integrator records the baseline capture helper's release-label
mistake separately; the pinned executable and visible UI are version 0.23.

## Test and cross-review record

The implementation uses real native `Activate` observers, the UI queue, the
interaction dispatcher and unchanged domain commands in its regressions. The
fixtures cover same-frame chooser changes, explicit missing targets, restore,
player/body/ownership transitions, changing health, cancellation, token replay,
exact desired state, item/amount/direction, loaded handover and ordinary rental
countdown. Shared management-focus coverage verifies safe Cancel entry, no
replayed opening acceptance, current confirmation eligibility and return focus.

Retained intermediate runs are not acceptance evidence:

- `player-native-focused-attempt1.log`: compilation rejected an ambiguous test import (`Command` also names a Bevy trait); changed to an explicit domain-command import.
- `player-native-suite-attempt2.log`: compilation found one stale local identifier after the generic inspector-focus rename; corrected it.
- `player-native-suite-attempt3.log`: 244 passed, 4 failed. Two new Build fixtures overlapped a living actor; the test now creates its real fence 12 m away. One older Fleet regression still expected automatic fallback; it now proves the explicit missing choice remains until deliberate browsing and a separate named review. One Frontier comparison fixture changed a public requested item without receiving demand; its recipe now authorizes the same request, preserving the original guidance assertion.
- `../v0.24-preflight/player-native-suite-attempt4.{json,log}`: 249 passed, 1 failed. The newly rented vehicle was close enough to Town interchange that the countdown fixture also picked up passengers before its queued Pause. The reviewed load correctly changed. The fixture now parks the real rental away from both stops so its countdown-only expectation is isolated; loaded-trip changes remain covered separately. All 390 captured implementation inputs were unchanged through this run.

The integrator's independent review found full `VehicleRental` snapshots included
the continuously changing countdown. This was corrected to stable lease terms,
with a real rent/assign/run/pause progression regression and expired-run rejection.
The source was then frozen for the reliability specialist's independent review.

Both independent source reviews closed with no remaining blocker:
`reliability-cross-review.md` and `world-player-cross-review.md`. The player's
independent review of supply and archive delivery is in `player-cross-review.md`;
it includes an independently executed 11-test archive suite, all passing.

### Full native acceptance

The corrected full suite passed with **250 passed, 0 failed, 0 ignored**, exit 0:

```sh
python3 /tmp/terra023-toolchain/run.py cargo test --offline --locked -p terra-demo --bin terra-demo
```

Execution started `2026-10-30T11:42:16.850980+00:00` and finished
`2026-10-30T11:43:38.339519+00:00`, with 81.533 seconds including compilation.
The tests themselves reported 2.20 seconds. The bounded runner recorded complete
output, no overflow and an absent owned POSIX process group after completion.
All **390 implementation inputs** matched their captured pre-run hashes.

Retained exact evidence:

- `../v0.24-preflight/player-native-suite-attempt5.json`
- `../v0.24-preflight/player-native-suite-attempt5.log`
- `../v0.24-preflight/player-native-suite-attempt5-inputs.json`

The ten actual native operation regressions cover Industry, Fleet cash/depot,
assignment/running/hold/release, real rental countdown/expiry, real loaded
handover, Build damage and missing faction recovery. Existing quote, destructive
review, save/recovery, focus, gamepad and wheel checks passed in the same suite.

## Source ownership and integration

New native modules: `reviewed_action.rs`, `industry.rs`, `building_review.rs`,
`fleet_operations.rs` and `native_operation_tests.rs`, all under
`crates/terra-demo/src/`. The Fleet operations module is nested under Fleet;
the others are declared through `main.rs` or the interface test module.

Existing integration edits are in `interface.rs`, `interaction.rs`, `fleet.rs`,
`communities.rs` and `command_input.rs`. The only Frontier change in this work
stream aligns an old test's receiver demand with its authored request. No stored
world, command or network schema was added or changed.

## Execution status

WP03–05 implementation, focused regressions and independent source review are
complete. Native source is frozen and the Cargo slot is released to the
integrator for the executable build, candidate/final native captures and
release-wide canonical acceptance. This record does not claim those later
visual or release results, physical-device qualification, Windows execution or
200% reflow.

## Candidate visual review and final copy correction

Independently opened the fresh `43-candidate-minimum-property.png`,
`44-candidate-minimum-damage-review.png` and
`47-candidate-minimum-fleet.png` in `../v0.24-native/`, each at 1100×700 with
150% text. This is candidate evidence, not the final confirmation round.

Capture 43 clearly names fence #2 and health 40/40. Its old “Review damage to
named structure” label repeats that context and wraps over two lines. Capture
44 keeps the full named 40 → 15 health consequence, no-refund explanation,
Cancel message and both choices visible. Lower inventory content reaches the
ordinary scroll boundary; no review content is lost. Capture 47 shows the Fleet
section chooser and explicit garage-within-20-m requirement for disabled
registration. It supports manual garage navigation, not an automatic travel
claim. No additional material visual defect was found in these three images.

After the integrator recorded the candidate's clean exit 0 at 12:32:09 UTC with
unchanged source, the final bounded copy batch changed only these two labels in
`crates/terra-demo/src/interface.rs`:

- “Review damage to named structure” → “Review damage”
- “Confirm named damage” → “Confirm damage”

The exact target, consequence, cancellation wording and context/action logic are
unchanged. No new test or Cargo run was added for these two text edits. The
integrator owns the final executable build, visual confirmation and canonical
250-test native gate. Native source is frozen again after this correction.

## Final native focus/text recovery correction

The fresh confirmation round exposed a functional rendering defect after the
copy batch. Independently viewed captures 208/209 show the complete named
15 → 0 damage consequence disappearing after command focus, with Cancel still
outlined and the paragraph's vertical space retained. The repeated captures
219/220/221 narrow the failure: 219 shows readable inventory, home and section
heading after Cancel; 220 loses all those non-button inspector paragraphs after
Escape; 221 restores text after scrolling. Root's repeated focused captures
216/217 remained readable. These pre-correction images remain evidence of the
defect; a successful earlier screenshot does not invalidate the failing states.

Source review found no intentional focus branch that hides those paragraphs.
`building_review::review_label` returns the complete current or expired review
whenever a review exists. `update_labels` already avoids unchanged string writes,
and text scaling already avoids unchanged font writes. Button styling changes
only button children. The missing static section heading additionally rules out
a failure confined to the dynamic damage label.

Bevy 0.20's retained extraction contract is documented in the primary
[0.19 → 0.20 migration guide](https://github.com/bevyengine/bevy-website/blob/main/content/learn/migration-guides/0.19-to-0.20.md#ui-rendering-is-now-retained).
The exact pinned `bevy_ui_render-0.20.0/src/lib.rs` implementation includes
`Changed<TextColor>` in `extract_uinode_changes`, discards that node's cached
render record, then reconstructs text in `extract_text_sections` using its
existing glyph layout, current transform and clipping. This supports requesting
re-extraction without rewriting text or changing layout. It does **not** prove
the exact upstream renderer cause of the observed failure.

The bounded correction changes only `interface.rs`: after `UiSystems::PostLayout`,
an actual transition into or out of command focus marks inspector paragraph
`TextColor` as changed. Static headings and the named review are included;
button children, the world panel, footer and non-text nodes are excluded.
The first observation and every stable-focus frame do nothing. Authored colors,
strings, font/layout data, scroll position and review consent are not changed.
This is not a per-frame refresh of all UI.

One focused regression covers repeated transitions in both directions, exact
change-detection scope and stable-frame no-ops, plus retained text/color/style,
computed geometry, scroll position, a real named damage confirmation token and
world state. The first compile attempt caught a fixture argument type mismatch
(`String` passed to the existing `&str` text helper); that one argument was fixed.
Its failed receipt and complete log remain as `player-focus-refresh-01` in
`../v0.24-preflight/`.

Focused run `player-focus-refresh-02` passed with **1 passed, 0 failed, 0 ignored,
250 filtered out**, exit 0. It ran from `2026-10-30T12:55:34.329596+00:00` to
`2026-10-30T12:56:54.939773+00:00`, 80.658 seconds including compilation. The
runner retained complete output, observed the owned process group absent, and
confirmed all 390 implementation inputs unchanged. Its `.json`, `.log` and
`-inputs.json` files are in `../v0.24-preflight/`. The independent reliability
source review found no blocker and is recorded in `final-visual-review.md`.

The repeated actual native no-scroll reproduction still determines visual
correction acceptance; the earlier 250-test pass predates this addition and
cannot substitute for the final 251-test gate. The integrator assigned the
frozen source to executable build `native-final-build-02` before that repeat.

That main executable build passed with exit 0 at
`2026-10-30T12:58:18.275776+00:00` (59.703 seconds). All 390 implementation
inputs remained unchanged and match the focused regression's input map. The
complete log and bounded execution receipt are retained under that build name
in `../v0.24-preflight/`. Its binary SHA-256 is
`092484f4ffbfc9f89b4950f7b55a935864117e5a7c96e83744224e07f53cb87d`.
Source is frozen and the Cargo slot is released to the integrator; repeated
native pixel confirmation and canonical release acceptance remain separate.

## Withdrawal: the alleged disappearing-text defect was a false visual assessment

**This section supersedes the preceding “Final native focus/text recovery
correction” assessment and its claim that the screenshots demonstrated missing
paragraphs.** The original images contain the text. The integrator compared the
exact RGB regions, and the player-experience reviewer independently reproduced
all four comparisons directly from the retained PNGs without modifying them:

| Actual captures | Compared rectangle `(left, top, right, bottom)` | Result |
| --- | --- | --- |
| 208 / 209 | `(795, 210, 1073, 386)` | Identical, 146,784 RGB bytes |
| 219 / 220 | `(795, 213, 1073, 402)` | Identical, 157,626 RGB bytes |
| 220 / 310 | `(795, 213, 1073, 402)` | Identical, 157,626 RGB bytes |
| 310 / 311 | `(795, 213, 1073, 402)` | Identical, 157,626 RGB bytes |

The 208/209 review-region SHA-256 is
`193adaf1435e4a4a56e0af80da2164fc49bc57fb150dc8292232faf1c75735ae`.
Every compared prose region in 219/220/310/311 has SHA-256
`effd3954b39bec9c15f5730757302f7f167274c584458f9e3a91b037f97f50ef`.
Independent reopening of 310 and 311 also showed all inventory, home and
section-heading text. Their whole-file hashes match the original capture
receipts. The earlier visual interpretation was incorrect; it supplies no
evidence of a disappearing-text defect or a required renderer workaround.

After the experimental native process exited cleanly, the experimental system
registration, transition helper and its regression were removed exactly.
`interface.rs` is restored to SHA-256
`36c1e855a5e5b93c3809e60b898514601e6fdcb217325b53c250b17d382b22be`,
retaining the useful “Review damage” and “Confirm damage” copy changes. The
resulting complete 390-input map is identical to `native-final-build-01`.

All experimental attempts, receipts and logs remain preserved, including the
successful focused regression and build02. They establish what those attempts
executed; they do not establish a native rendering defect or a visual fix.
The final source contains **250 native tests**, with canonical acceptance owned
by the integrator. No extra test was added or rerun for this exact reversion.
The rebuilt final executable will be recorded as `native-final-build-03`.

Rebuild03 completed with exit 0 at `2026-10-30T13:09:39.021005+00:00` after
63.058 seconds. Its complete output and bounded execution receipt confirm the
owned process group is absent and all 390 source inputs remained unchanged.
The rebuilt binary SHA-256 is
`509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`,
**exactly equal to build01**, and the source-input maps are equal as well.
The integrator owns the final native evidence selection and canonical
250-test gate. Source remains frozen after this exact restoration.
