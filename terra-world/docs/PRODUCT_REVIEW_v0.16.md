# Product review and improvement pass — 0.16

Review date: **2026-10-29**. The accepted [0.15 product review](PRODUCT_REVIEW_v0.15.md)
is preserved exactly. The [current gap register](REVIEW_GAPS_v0.16.md) retains
G01–G18 and the wider product conditions that this bounded increment does not meet.

**Automated and native acceptance passed.** All **43 final gates** passed on **366 unchanged implementation inputs**, with **904 distinct Rust tests**, **990 passing test executions**, zero failed and zero ignored. Four native confirmations used the exact accepted executable and exited 0. [Validation](VALIDATION.md) records the commands, measurements and limits; [native](native-v16-verification.json), [desktop](desktop-package-v0.16.json) and [release](release-verification-v0.16.json) receipts bind the completed results.

## Product outcome

This increment connects four parts of the regional reference game that caused
practical friction. Players can review their accepted work, deadlines, purchase
costs and other commitments. A vehicle can recover from the declared narrow
supported-road obstruction. An original animated human retains saved character
choices and fits a deliberately authored cabin when the local owner drives.
Recovered worlds can be named before restoration and renamed afterward.

These changes use existing ownership, economy, physical movement and persistence.
They do not award money for reading guidance, erase a blocked commercial order,
move a saved avatar into a visual seat, or replace existing recovery copies.
The unchanged save/regional/wire contracts are documented in
[Compatibility](COMPATIBILITY.md). Optional human/seat asset fields need a 0.16
reader; older packs that omit them retain their identities.

Three implementation subagents owned guidance, navigation and content. The
release owner implemented recovery naming and integration. Reviews crossed those
ownership boundaries, followed by native interaction and a separate correction
batch. Their [reports](verification/v0.16-review/) distinguish source findings,
focused checks, candidate screenshots and final acceptance.

## What native review found, and what changed

The completed candidate driving and recovery processes used executable SHA-256
`90448c7b042d2d87f811dfbe442005543e696c67f898ff8a9d64db97c1be94db`.
Both exited **0**, with 366 implementation inputs unchanged during each run.
The two process receipts are [driving](verification/v0.16-native/v16-candidate-driver-process.json)
and [recovery](verification/v0.16-native/v16-candidate-recovery-process.json).
They are expert-operated scenarios with prepared later-state fixtures, not an
uncoached player study or a hardware-speed measurement.

| Observed problem | Correction | Why it matters |
|---|---|---|
| Reviewing a request changed the guide above the scrolled viewport. | A successful explicit Review reveals the inspector guide and names the opened request. Explicit goal selection does the same; automatic refresh preserves reader position. | The result of a deliberate action is visible without searching. |
| Purchase advice repeated seller, price, stock and wallet details, while the accepted-work step omitted the gross reward. | Show one gross reward/XP and complete missing-quantity comparison. Preserve the actual action seller and recipe-ingredient advice. | Players can compare the displayed commitment and its purchase cost without interpreting duplicate paragraphs. |
| Exit changed the displayed car from nearby Comet #1000073 to a different owned truck 201 m away. | Pin the already displayed vehicle before the ordinary authority-controlled Exit. Preserve an explicit other selection. | Enter remains discoverable beside the car just left. |
| A hardcoded shortcut list repeated the configured driving help. | Keep actor/view status concise and let the existing configured help provide movement keys. | Remapped input has one source of instructions. P is a valid default parking-brake binding; Space remains held handbrake. |
| Route guidance showed a number beside a turn without explaining its meaning. | Label it “m remaining.” | Remaining route length is distinguishable from direct destination distance. |
| Recovered copies were difficult to distinguish later. | Review an editable destination name before recovery, allow safe title-only Rename, and display creation time to seconds. | A player can identify the intended copy without relying on temporary focus. |

The [candidate driving review](verification/v0.16-review/native-driving-polish.md)
and [guidance/viewport review](verification/v0.16-review/native-cross-review.md)
retain the actual screenshots and the implementation rationale. An early suspicion
that P lacked a handler was withdrawn after source inspection confirmed its
configured parking-brake path. The remaining help issue was real and corrected.

## Guidance, economy and sustained play

The six goals are Suggested, Supply, Home, Community, Road and Company.
A direct chooser removes the need to cycle through unrelated branches.
**Your commitments** gathers the local owner's accepted supply requests,
personal money, assigned road work, company funds and loaded obligations, and
rental bonds. It is a bounded projection of authoritative state, not a second
progress ledger. Earliest world-clock deadline and then stable ID determine the
default accepted request; a deliberate accepted selection retains priority.

Review is independent of travel and delivery. It is bound to the full request
actually displayed and the current principal/restore context. Noticeboard selection changes and accepted-request review
invalidate the queued commitments context until the new labels are rendered.
Cross-review corrected a queued-selection case that could otherwise review a
request the player had not yet seen.

The cost comparison prices all missing units from a seller with sufficient
stock. It includes gross reward and XP, missing-input price, wallet shortfall
and a warning when the compared purchase exceeds the reward. Carried goods and
travel are unpriced. The current price comparison reserves neither stock nor funds and does not
prove capacity, reach or affordability; the actual transaction rechecks those
conditions. Recipe advice can still send the player to buy an ingredient while
the whole-quantity comparison prices ready-made goods from another seller.

The native candidate accepted two requests, exposed both commitment rows and
reviewed the exact selected work. A [whole-state comparison](verification/v0.16-native/v16-candidate-guidance-review-comparison.json)
found identical authoritative state, identity, player and save bytes before and
after Review. The 150% text journey at 1100 × 700 reached all six goal choices
and the commitment actions through the normal inspector scroll.

The deterministic continuation regression separately accepts three requests,
returns one, expires one and physically completes one. It compares saved/resumed
continuation and preserves ownership/value rules. Advancing a declared world-day
window is separate from physical walking time. This establishes continuity for
the scenario; it cannot establish career balance, first-time comprehension or
an enjoyable long session. G03–G04 retain those closure criteria.

## Autonomous movement and physical credibility

A reproducible 0.15 case oscillated behind a parallel parked vehicle on a
9.5-metre supported strip instead of completing an unchanged 60-metre trip.
The new fallback uses the actual parked body's lateral extent. If already too
close to turn, it can reverse physically, take two corners and reconnect.
Every new narrow/cluster leg includes complete source-envelope support,
traffic and body-sweep admission. Additional corridor proposals stay within
12 admissions per vehicle update, shared across all substeps and retries.

Full release verification also found two established warehouse careers stalled
after six deliveries: a roughly 154-metre reconnection fit the new outer bound
but exceeded the compatibility callback's existing 512-interval limit. The
[career regression review](verification/v0.16-review/navigation-career-regression.md)
records the reproduction, segmented full-envelope correction and preserved
career assertions. Two additional regressions cover a 155-metre trip and failure
in the final segment, including exact resumed continuation.

The geometry matrix covers the narrow parked obstruction, temporarily absent
then restored flank support, unavailable reconnection, two moving crossings and
a blocked narrow junction. The legacy single-body reconnect source check now
rejects distances over 160 metres before querying support. A complete admitted
reconnection is split into source calls bounded by 64 metres of translation
plus rotational envelope travel, with 0.1 metre of numerical/support tolerance.
Each call consumes the same shared twelve-call allowance; no partial route is
committed if a later segment fails. Its future traffic
occupancy remains checked during physical stepping: admitting all future
occupancy at planning time broke established loaded-courier behavior and was
rejected during integration.

A failed proposal retains the original route and values. Accepted maneuvers may
reclaim consumed Move prefixes without deleting remaining work or destination.
Patrol continuation, bounded steps/turns, inventory/money conservation and exact
resume are regression conditions. Moving, routed and crosswise bodies cannot
seed the new narrow proposal.

These are authored flat SDK scenarios with a frozen calendar. The 600-second
support-loss case makes repeated safe clearances; it is not a stationary,
zero-cost wait. The blocked-junction case allows an unrelated exit and resumes
after an explicit Move command physically clears the blocker; it does not
establish autonomous global rerouting. Goods/pocket-money conservation is not a
measurement of operating fuel, commercial profit or real-road throughput.
See [navigation review](verification/v0.16-review/navigation-implementation.md)
and [Validation](VALIDATION.md) for exact results and limits.

## Character, animation and the production pipeline

The optional original **Fieldhand** provides idle, walking, running and seated
clips. Three body styles and eight skin/hair/outfit choices retain their meaning.
The importer requires three unique, used, opaque white customization roles with
no base-color texture, vertex color or emission. Each rendered human owns three
material instances; updates reuse them, and teardown releases them. A runtime
primitive-role check keeps the procedural human visible until replacement is
actually ready. Actual admitted movement drives locomotion; pause freezes clip
phase and blend, including paused initial idle/seated setup.

Independent review found a color-space mismatch: the saved palette was sRGB,
while the imported materials had treated it as linear light. The correction uses
the same conversion as procedural vertices and compares their actual colors.
The first integrated test run failed that assertion; its log is retained, and
the corrected integration run passed all 158 then-current native tests. Later
polish adds further regression coverage, counted only in the final gates.

A model-specific driver-seat point applies only to a known manual owner whose
active registered vehicle and avatar agree. Presentation follows the vehicle's
interpolated terrain alignment; saved avatar authority does not move. Public
remote appearance does not imply occupancy. First person hides both rider and
exterior; Exit/control release follow existing authority semantics.

The original Comet shell did not fit the seated person. A separate original
cabin variant corrects the roof, floor, shell and seated joints while retaining
the earlier Comet asset exactly. Review sampled 49 animation times for each
body style. Minimum recorded roof clearance was 36.98 mm and sole/floor
clearance 10.64 mm. Those authored-pose measurements are not IK or dynamic
hand-contact guarantees. The [geometry audit](verification/v0.16-review/content-cabin-after.json)
and [content review](verification/v0.16-review/content-cross-review.md) preserve
the checks and their scope.

If the cabin is pending or failed, its procedural car stays visible and the
rider is hidden because that closed fallback cannot fit the human. Once the
cabin is ready, the human fallback is independently visible until the imported
human is ready. Native side-window and first-person captures confirmed the
intended composition in the candidate. The seven-model composed pack combines
Fieldhand, the cabin and five unchanged wildlife models. Its generator reproduces
two GLBs and three JSON declarations/recipe files; its recipe exercises immutable publication and
validation. The original woodland pack remains the default.

## Recovery, operations and trust

Recovery review validates the chosen name before allocating output or starting
the worker. The progress message leads with the destination name, then identifies
the source backup/checkpoint. A successful restore publishes a separate complete
managed world and leaves Open as a separate action.

Rename validates the complete selected profile, revision and direct managed
path before atomic title replacement. Stale/cancelled edits, invalid names,
external paths, symlinks and hardlink aliases are rejected. No backup, checkpoint,
source input, character setting or creation/last-played timestamp is changed.
Renaming an unplayed world does not invent a checkpoint. Equal titles or
second-resolution timestamps remain legal; the managed directory is identity.

At 150% text, candidate Rename cancellation left all 43 original fixture files
unchanged. Applying “Valley - retained copy” changed exactly the profile title.
Named recovery created “Valley - autumn recovery” without opening it. Its
[pre-open comparison](verification/v0.16-native/v16-candidate-recovery-exact.json)
verified every SQLite definition, column and typed row: 19 region rows and one
world row, plus exact source/political/catalog dependencies and preserved
character/quality/start-hour values. The original world and source tree remained
absent. Larger regional storage and TLS workloads are separate automated evidence.

## Confirmation after the correction pass

Four subsequent native processes used the exact executable from the final gates
and each exited normally. Their [final review](verification/v0.16-review/final-native-review.md)
and [acceptance record](native-v16-verification.json) retain the observed results,
capture identities and limits separately from the earlier candidate evidence.

At 1100 × 700 and 150% text, selecting Company revealed its next action;
Review named accepted request #2000000 and revealed the Supply instructions.
The before/after paused save remained byte-identical. For the second request,
one scrolled view showed 5.47 credits gross reward plus 40 XP beside the
4.48-credit price of all four missing wheat, with the unpriced travel/carried
goods and live-quote caveats. See the [goal handoff](screenshots/v16-confirm-guidance-selected-goal.png)
and [financial comparison](screenshots/v16-confirm-guidance-price-comparison.png).

The final driver views show the person within the ready cabin, intentional
first-person hiding, a single configured control hint and an explicit remaining
route distance. Exit retains Comet #1000073, 2 m away, with Enter enabled;
re-entry controls the same car. These are one-appearance static views alongside
the geometry/runtime regressions. The status phrase “Comet hatchback rental ·
Worker” after re-entry remains a minor copy refinement; the control line
correctly identifies DRIVING. See [cabin fit](screenshots/v16-confirm-driver-cabin-clear.png)
and [retained selection](screenshots/v16-confirm-driver-exit-selection.png).

Named recovery created “Valley - final review” as a separate copy. Before
opening, every column and row in the 19 saved regions and one world record
matched the backup. All 57 existing fixture files remained unchanged before
and after opening. The new copy then opened with 49 ready terrain chunks and
no reported runtime/persistence errors. Final presentation used 64 imported
instances at the 64-instance limit, with nine budget-deferred and 18 distant
actors retaining procedural presentation. All selected and reserved work stayed
within the configured limits, with no loading, failed, rejected or unmetered
instances. This demonstrates bounded fallback in a larger scene; it does not
measure hardware residency or qualify a production deployment.

## Review across product perspectives

| Perspective | Evidence and improvement | Remaining qualification |
|---|---|---|
| First-time/returning player | Direct goals, owner commitments, exact Review and visible handoff. | Uncoached comprehension, sustained content and setbacks. |
| Economy/progression | Whole missing-quantity price comparison, gross reward/XP, wallet/bond separation, real transactions. | Longer beginner/advanced strategies, living/capital costs and replenishment. |
| Interaction/information design | Review scroll reveal, concise cost/help text, route units and retained vehicle selection. | Broader novice studies and dense-scene interaction. |
| Accessibility/localization | Actual small-window 150% review, wrapping and editable names; automatic refresh preserves scroll. | Assistive access, keyboard/controller/wheel qualification, contrast/motion and languages. |
| Visual/content quality | Original custom-colored human, fitted cabin, pause/visibility/fallback policy. | Production rigs, facial/equipment/passengers, sound and target-device LODs. |
| Simulation/navigation | Shared bounded handling, full new-corridor checks and exact continuations. | More geometry, traffic density and long-running interactions. |
| Security/authority | No new reward/ownership/save authority; strict stale-context and managed-path checks. | Public account/operator/moderation and trusted distribution controls. |
| Reliability/recovery | Separate named copies, atomic rename, database/dependency comparison and reopening. | Deployment retention, offsite/confidentiality policy, recovery owners and RPO/RTO. |
| Performance/resource use | Bounded proposals/import work, material lifetimes, preserved regional workload gates. | Measured hardware frame tails/residency and storage-pause distributions. |
| Multiplayer | Existing protocol/TLS and loaded-authority gates retained; remote appearance not occupancy. | WAN/capacity soak, operational policy and remote seated occupants. |
| Developer workflow | Reproducible example/recipe, validation, compatibility and corrected source guides. | Broader DCC presets, migrations and licensed shipping data. |
| Delivery/maintenance | Frozen inputs, preserved baseline/history, extracted desktop smoke and hashed evidence. | Windows clean-machine signing/install/update/removal qualification. |

## Final acceptance and delivery

Final gate counts, exact executable confirmation, extracted-package results and
artifact identities are recorded in [Validation](VALIDATION.md),
[native acceptance](native-v16-verification.json),
[desktop acceptance](desktop-package-v0.16.json) and the
[release receipt](release-verification-v0.16.json). These records are published
only after their checks complete. The full source ZIP contains the report,
commands, logs, screenshots and retained history. The separate Linux review ZIP
contains the accepted executable, runtime inputs, current player guides and
accepted automated evidence; developer/history links refer to the source ZIP.

The [complete gap report](REVIEW_GAPS_v0.16.md) sets the next acceptance criteria.
This increment closes the recorded local defects and improves four structural
areas. It does not turn the regional reference into a globally qualified game,
a signed cross-platform consumer release or a production Internet service.
