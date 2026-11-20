# Product perspectives — 0.17 reviewer note

Review date: **2026-10-29**. Review cut: implementation frozen, final automated
gates complete, candidate native evidence reviewed; frozen native and extracted
package confirmations are still in progress. This note does not accept those
later results in advance.

The [final gate record](../v0.17-final/gates.json) records **43 passed gates,
928 distinct Rust tests, 1,014 passing executions, zero failures and zero ignored
tests**, with **367 unchanged implementation inputs**. Counts were independently
rederived from the retained harness output. The demo suite includes all **181**
tests, including the reviewed driving and terminal-menu regressions. The gates
bind executable SHA-256
`a6a6b6c1fd840aa448a73e6925b0d2e79ee2d8602fb47ef3af169d7eb44a0d44`.
Headless correctness, expert-operated native observations and uncoached player
experience remain distinct evidence categories.

## Player guidance, gameplay and economy

The six existing goals and owner commitments give first and returning players
an actionable starting point without adding a second progress ledger. Accepted
supply work retains its exact reviewed identity and earliest world-clock
deadline priority. Advice compares the gross reward against all missing units
at a fully stocked visible seller, discloses shortfalls and leaves carried
goods, travel and time unpriced. It neither reserves a market exchange nor
promises profit. The current continuation and three priced-career gates passed;
they establish declared finite strategies and conservation, not long-session
balance or novice comprehension. The changes preserve existing prices,
deadlines, ownership and command authority. Sources:
[guidance](../../GUIDANCE.md),
[continuation](../v0.17-final/guidance-continuity.json),
[career](../v0.17-final/career-report.json),
[purchased career](../v0.17-final/purchased-career-report.json) and
[novice career](../v0.17-final/novice-career-report.json).

## Driving and physical interaction

Interact now resolves the active vehicle's accepted assignment, retaining its
identity, service phase and deadline even after another offer is browsed. A
restore or phase change cannot reinterpret the queued stop. Rental return
retains the rendered principal, restore context, vehicle, garages, bond and
full refund breakdown; confirmation must become visible and is consumed once.
Domain reach, motion, ownership and quoted-value checks remain authoritative.
Focused regressions cover stale reviews, duplicate input and implicit vehicle
fallback. Binding-aware service hints and the shared vehicle status formatter
remove contradictory control guidance. See the
[implementation and test review](player-journeys.md).

The candidate post-F9 image visibly reads **Driving · Third person**, consistent
with its DRIVING footer. Garage shows the selected rental 85 m from the garage
and a clearly itemized **61.80 cr acquisition offer**. The fixture was active and
away from its garage; it was not an accepted-job demonstration. **No native
accepted-job delivery, actual refund or rental-return confirmation is claimed.**
Those handoffs retain source/headless evidence. Existing traffic and navigation
gates also passed, but finite narrow-road and obstruction cases do not qualify
arbitrary junctions or dense traffic.

## Developer assets, terrain and content production

The retained authoring workflow inspects bounded self-contained GLBs, computes
content identities and publishes a new validated pack directory. Wildlife,
Comet and customized Fieldhand generation/inspection/build/validation gates
passed. The prepared-source inspect/export/install/verify chain also passed for
the two connected georeferenced bundles. This supports reproducible local
iteration and explicit source coverage; it does not supply a production catalog
or real-world survey. Asset schema 1, domain/regional schema 1 and wire 10 remain
unchanged. Optional human/seat fields retain their existing 0.16 reader
requirement. See [authoring](../../ASSET_AUTHORING.md),
[compatibility](../../COMPATIBILITY.md) and the
[source inspection](../v0.17-final/source-inspection.json).

Remaining content work includes broader rigs/retargeting, equipment, boarding,
remote occupants, audio, DCC presets and target-GPU budgets. Wider terrain
shipping needs licensed data, reproducible ingestion and declared handling of
coverage, datum, topology and bridges/underpasses; cold saved records do not
extend loaded terrain.

## Safety, reliability and recovery

Terminal worker failure now clears false pending status while retaining the
last acknowledged state and blocking further work until validated reopen. A
real SQLite regression establishes the central uncertainty: a generation can
commit before its acknowledgement disappears. Reopen recovers that generation
before a later save; the old session cannot report a successful idle barrier.
Ordinary completed SQLite failures still permit retry. Local terminal Loading
and Session surfaces expose **Close failed world**, preserve the unconfirmed
progress explanation and use the existing error-exit/handoff path. Normal save,
restore, retry and acknowledged quit remain covered. Sources:
[worker review](reliability-implementation.md),
[menu regressions](player-journeys.md),
[recovery probe](../v0.17-final/recovery.json) and
[complete recovery bundle](../v0.17-final/recovery-bundle.json).

There was **no native worker-loss fault injection** in this review. Worker-loss
tests and menu-policy tests establish different parts of the contract. Final
storage operations can still wait on the filesystem; these repairs do not set
a disk-latency bound, deployment retention policy or RPO/RTO.

## UX, accessibility and visual clarity

The player-facing corrections are small and concrete: accurate vehicle role,
configured Interact hints, explicit blockers, retained reviewed terms and an
actionable terminal exit. Diagnostics use existing Settings, pause and F12
surfaces. Independent candidate review found no material visible blocker in the
supplied 1440 × 900 and 1100 × 700 frames, including Settings and pause at 150%
text. The lower pause action has a visible keyboard-focus outline; the root
operator separately reported the Tab/Enter save-and-return interaction.
Screenshots establish visible states, while process and saved-state claims need
their separate receipts. See [diagnostic UI review](diagnostics-native-ui-review.md)
and [driver images](player-journeys.md#fresh-native-inspection).

The complete unobscured F12 destination and frozen native confirmations remain
pending at this cut. Physical controllers/wheels, assistive technology,
measured contrast, motion comfort, localization and representative-user
accessibility are unqualified. No screenshot review substitutes for those
journeys or for uncoached discovery and recovery.

## Performance, privacy and operations

Local reports are default-Off and apply to sessions subsequently launched from
the menu. Save defaults persists consent; graphics-only updates preserve it.
Capture starts with the first running update, then uses ten-real-second
intervals and a final attempt. Sixteen samples, 64 KiB per sample, a 2 MiB file
and one active plus one queued write bound collection. Actual virtual pause,
calendar pause, queued saves, checkpoint state and report publication remain
distinct. The OS-lifetime lock, interruption recovery and unrelated-path
protections have Linux regressions. Stale or absent reports establish neither
current health nor a clean exit; an exit sample also needs an independent
process receipt. The report omits raw saves and private gameplay payloads, but
error strings can contain paths/source details and need review before sharing.
See [diagnostic implementation](diagnostics-implementation.md).

The [current regional workload](../v0.17-final/regional-sessions.json) uses 16
real TLS socket clients, eight churn cycles, 25 resident prepared-source chunks
and 1,024 cold overlay regions, with exact conservation/resume/recovery checks.
It has no renderer or Internet delay/loss. Shared-container timings and finite
memory samples do not establish physical-GPU performance, production player
capacity, a long-duration memory plateau or dense-world playability.

## Onboarding, delivery and next evidence

[Playtesting](../../PLAYTESTING.md) now supplies setup, an assisted first/returning
route, focused follow-ups and an issue template. It distinguishes that script
from uncoached observation and explains opt-in collection, saved progress and
terminal uncertainty. Candidate menu evidence binds the enabled preference,
launched child, report path and matching session; it remains
[candidate evidence](../v0.17-native/candidate-menu-handoff-completed.json).
Frozen native, extracted-package and final preservation receipts must be
completed separately before delivery is called accepted. Windows remains
unexecuted: Linux, source archives and configured CI do not qualify a Windows
executable, clean-machine installation or target hardware.

The highest-value next evidence is Windows/hardware create/play/save/reopen/
recovery, uncoached new and returning journeys, and longer declared economy and
traffic scenarios. Public hosting additionally needs enrollment, operator,
moderation, WAN and deployment recovery qualification. These boundaries remain
in [G01–G18](../../REVIEW_GAPS_v0.17.md); this increment closes specific failures,
not every condition in those product areas.

## Documentation cross-check and applied polish

The current 0.17 gap register, guidance and playtest recovery explanations agree
with the reviewed contracts. Desktop troubleshooting previously directed every
blocked save toward retry/recovery without distinguishing a stopped worker.
With release-owner approval, [Desktop delivery](../../DESKTOP_DELIVERY.md) now
distinguishes recoverable retry/restore from local terminal close/reopen and
links the 0.17 gap register as its current follow-up. No implementation,
configuration, script or hashed compatibility document changed in this pass.
The release owner is replacing the generic Product Review and Validation pages
after frozen native evidence; their temporary 0.16 contents are not used as
0.17 acceptance here.
