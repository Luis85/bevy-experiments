# Playtest readiness: improvements and open gaps — 0.17

Review date: **2026-10-29**. This increment starts from the exact accepted
[0.16 project](REVIEW_GAPS_v0.16.md). Its current product review and validation
are preserved as [PRODUCT_REVIEW_v0.16](PRODUCT_REVIEW_v0.16.md) and
[VALIDATION_v0.16](VALIDATION_v0.16.md). Stable **G01–G18** identifiers continue
from the [0.12 register](OPEN_GAPS_REPORT_v0.12.md).

The goal is a dependable regional reference for playtesting and further
iteration. This pass closes concrete failures in storage-worker recovery,
reviewed driving actions and collection of useful local evidence. It adds a
practical [playtest handoff](PLAYTESTING.md), then reviews the combined behavior
from player, design, accessibility, economy, technical, operational, content and
delivery perspectives. [Validation](VALIDATION.md) is the authority for completed
gate counts, native observations, package checks and their limits.

**Current acceptance:** all **43 final gates** passed with **928 distinct Rust
tests**, **1,014 passing executions**, zero failed and zero ignored, on **367
unchanged implementation inputs**. Five subsequent direct native confirmations
used the exact accepted executable and exited 0. They cover a named world,
save/reopen, configured driving controls, a 121-chunk larger region and retained
menu diagnostics. Fourteen observed captures include 1100 × 700 at 150% text.
The save/reopen comparison retains identity, inventory, money, ownership,
character choices and clock settings while generation advances 68 → 85.
See the [final native review](verification/v0.17-review/final-native-review.md)
and [acceptance record](native-v17-verification.json). Candidate creation and
opt-in interaction remain separately identified evidence. The completed desktop
and release receipts linked from Validation establish archive acceptance.

The domain save, regional schema and asset schema remain **1**; wire remains
**10** with ALPN `terra-world/10`. There is no economy migration, source-identity
rewrite, new account authority or hidden progression ledger. Optional human/seat
pack fields retain the reader requirement introduced in 0.16. Diagnostics add
observational session fields to the existing bounded envelope. See
[Compatibility](COMPATIBILITY.md) and [Diagnostics](DIAGNOSTICS.md).

## Gaps closed within this increment

| Finding | Implemented behavior | Why it matters for playtesting |
|---|---|---|
| A dead storage worker could remain pending forever. | Terminal failure clears false backpressure, latches the error and rejects further submissions, polling and maintenance until a validated reopen. Ordinary completed SQLite failures retain retry behavior. | Testers can distinguish work in progress from a session that cannot recover in place. |
| Clearing pending alone could conceal an uncertain commit. | A real lost-acknowledgement regression proves that a committed generation survives and is recovered before a new save. The old session never treats a later idle barrier as success. | Progress is neither declared lost nor saved without evidence; transactions are not blindly repeated. |
| Reopen advice had no matching session-menu exit. | Terminal local failures expose **Close failed world** through the existing final-checkpoint/error/handoff path. Normal recoverable errors still use retry/restore. | Recovery instructions can be followed with the product's own controls. |
| Rental confirmation retained only a boolean. | Return binds the rendered principal, restore context, vehicle/model, garage, bond and complete refund breakdown. It must be displayed before confirmation and is consumed once. | A changed selection, quote or queued double click cannot quietly submit different reviewed terms. |
| Driving Interact could follow an unrelated browsed offer. | It resolves the active vehicle's accepted assignment and retains the exact stop/phase context for the action. Motion, reach and missing-stop blockers are explained. | The contextual action follows what the player is actually driving. |
| Rebinding and vehicle re-entry produced contradictory help. | The service key follows Interact rebinding. Direct market opening avoids claiming the contextual key. Vehicle status remains a driving/selected-vehicle description after focus changes. | Controls and role feedback agree with the active context. |
| Evidence collection required direct command-line setup. | Main-menu Settings has a default-Off local report choice, fixed per-user-data destination and retained summary. Pause and F12 show successful capture age, pending work and errors. | A tester can enable and find bounded evidence through the normal menu. |
| Shared report paths could have competing writers. | A persistent marked sidecar uses an OS lifetime lock; ownership releases on process termination. Invalid or unrelated destinations are rejected. Final acknowledgement follows lock release. | Concurrent sessions cannot replace each other's report through the supported writer path; reopening does not depend on deleting a stale lease. |
| Busy, paused, failed and stale evidence could be misread. | Reports distinguish virtual pause, calendar pause/time scale, flow phase, queued save/restore, host ownership and committed generation. Stopped writers stop claiming pending work. Error notices retain separate storage and diagnostic sections. | A paused game or old sample does not appear to be a current healthy session, and report failure is not mistaken for save failure. |

## Persistence: uncertainty must remain explicit

The worker owns asynchronous regional operations. Its result can disappear after
the database has already committed. The former pending flag trapped callers in
an apparent busy state. Simply resetting that flag would have introduced a more
dangerous ambiguity: a later finish or maintenance barrier could have succeeded
against retained pre-completion state.

The new terminal latch covers disconnected request/results, poisoned result
access and mismatched completion identity. It retains the last acknowledged
status, closes submission and consistently requires reopening. Native and host
adapters keep authority blocked. The local UI supplies an exit, while the
database's validated committed generation decides what is resumed. A failed
final barrier produces an error exit rather than a successful-save claim.

Three focused tests use actual worker operations and SQLite state, with no
production fault-injection switch. The lost-checkpoint case checks the complete
committed archive and metadata at generation 2, then a validated reopen and a
successful generation 3 save. Lost hydration preserves the archive and retained
host view. Idle request-channel loss rejects future work without false pending.
Existing rollback/retry, contention, crash, corruption and backup tests remain.
See [worker review](verification/v0.17-review/reliability-implementation.md),
[worker implementation](../crates/terra-persistence/src/worker.rs),
[regressions](../crates/terra-persistence/src/worker_tests.rs) and
[operations](OPERATIONS.md).

This establishes a bounded failure contract. It does not measure arbitrary disk
stall duration, interrupt a wedged OS write, or replace deployment backup policy.
Final maintenance can still wait on storage. The terminal menu path has source
and regression coverage; native fault claims must be limited to the actual
failure scenarios retained in this release's native record.

## Player actions: retain what was actually reviewed

The economy remains finite and authoritative. This pass repairs presentation
handoffs that could choose the wrong transaction context before the existing
domain command evaluated it. Rental return now retains both the subject and the
complete displayed refund terms. A second queued input waits for a displayed
confirmation, a consumed review cannot be reused, and changed principal/restore/
vehicle/garage/quote forces review again. A routine prepaid countdown alone does
not invalidate unchanged refund terms. Host-side quoted-refund validation still
decides whether the transaction succeeds.

Driving Interact resolves an accepted job attached to the active vehicle instead
of prioritizing a separately browsed offer. Its exact service phase and stop are
retained, so an old pickup intent cannot silently become a delivery after a
restore or intervening action. A different offer remains available for browsing.
Tests exercise ordinary acceptance and boarding, a real externally completed
rental return followed by implicit selection fallback, and the relevant stale
actions. Explicit fixture positioning isolates the transaction handoff; those
tests do not establish human travel time or career balance.

The review also corrects fixed-key and role wording. These are small changes,
but they address information the player uses at every stop and re-entry. See
[journey review](verification/v0.17-review/player-journeys.md),
[driving implementation](../crates/terra-demo/src/driving.rs),
[controls](CONTROLS.md) and [game flow](GAME_FLOW.md).

## Diagnostics: usable evidence with clear limits

The menu choice is Off by default. It applies to the next world or host session
launched from that menu; Save defaults makes the choice persistent. Older
preferences default Off. In-game graphics updates preserve the diagnostic
choice, and malformed preference files are not silently replaced by a graphics
save. The report remains local; there is no automatic upload or external service.
The new optional launcher-preference field is separate from world-save
compatibility. Pre-0.17 preference readers can reject that unfamiliar field and
fall back to graphics defaults with a warning when downgrading; retain prior
preferences if those defaults must survive a rollback.

Capture is attempted during the first running update, then repeats every ten
real seconds with a final exit attempt. Startup can fail before any new report
is produced. Sixteen samples, 64 KiB per sample and 2 MiB per file bound
retention; one active write plus one queued write bounds work. Busy capture may
be skipped and is counted. The roughly 2.5-minute retained periodic tail is not
a full-session journal or crash dump. A real process-termination regression
checks OS lock release, alongside concurrent writer exclusion, path aliases,
unrelated-file preservation and final-acknowledgement ordering.

The visible state comes from completed publication, not merely enabling the
option. An old file or a running sample cannot certify current health or a later
clean exit. Session phase, actual virtual pause and calendar pause are separate
fields. Current checkpoint status remains distinct from report export status.
Summaries are bounded and read on menu entry or explicit Refresh, not parsed
every frame. The menu displays a path; it does not claim clipboard integration
or a file-manager button. The playtest guide explains how to collect the file.

Privacy regressions place sentinels in a private resume capability and typed UI
text, and assert that inventory/player payload keys are absent. Source review
also checks that the report's constructed fields omit balances and raw logs;
those omissions are not represented as separate executed sentinel tests.
Reports still include identifiers and error strings, which can contain local
paths or source information. A user should review an attachment before sharing.
See [diagnostics review](verification/v0.17-review/diagnostics-implementation.md),
[diagnostic contract](DIAGNOSTICS.md) and
[capture implementation](../crates/terra-demo/src/telemetry_capture.rs).

## Review and polishing across product perspectives

| Perspective | Review result and applied improvement | Boundary that remains |
|---|---|---|
| New and returning players | Existing six-goal guidance and commitments are retained; accepted road work now wins contextual Interact. The guide separates assisted verification from uncoached observation. | Human comprehension, motivation and continued content depth still need participants. |
| UX and visual design | Preserve the current muted world/menu design. New diagnostics sit in Settings and existing diagnostic surfaces. Pending, terminal and report-failure copy describes an actionable next step. | Visual acceptance is limited to the fresh captured sizes/states listed in Validation. |
| Accessibility and input | Dynamic Interact hints, clear role text and scrollable/focusable settings support existing keyboard and text-scale behavior. | Physical controller/wheel, assistive access, motion comfort, contrast and localization are not certified by simulated input. |
| Economy and ownership | Full reviewed rental terms and exact assignment context retain existing command authority. No prices, funds, rewards or progression schema change. | Long strategies, living/capital costs and balance require separate measured sessions. |
| Simulation and navigation | Existing physical handling, connected-source checks and bounded route recovery remain under release regression gates. | Arbitrary dense traffic and complex geography are not established by the finite fixtures. |
| Content production | Existing original wildlife, Comet and customized Fieldhand assets and immutable pack pipeline remain checked and preserved. | Production catalogs, retargeting, audio, remote occupants and target GPU budgets remain future work. |
| Reliability and recovery | Terminal worker uncertainty, actionable close/reopen and diagnostic writer ownership gain explicit contracts and regressions. | Disk latency tails, offsite retention and deployment RPO/RTO need a chosen environment. |
| Privacy and operations | Local opt-in reports omit saved gameplay/private identity payloads and preserve earlier output on refusal. Specific storage and export errors survive supervisor handoff. | Public account/moderation, external retention and alert ownership remain separate services. |
| Developer iteration and delivery | A repeatable playtest route, issue template, source/gate manifests and exact package receipts support the next iteration. All prior evidence remains historical. | Windows execution, signing, installer/update policy and clean-machine hardware qualification remain open. |

The subagents cross-reviewed one another's work. That process found the stopped
diagnostic writer's false-pending status, the terminal exit dead end, misleading
first-capture timing, generic error masking and truncation that could hide one
of two simultaneous failures. These were treated as integration defects and
corrected before acceptance. The combined native review and any additional
polishing are recorded in [Product review](PRODUCT_REVIEW.md).

## Complete remaining-gap register

“Scoped improvement” means a concrete condition is implemented and checked
within its declared boundary. It does not close the whole product area.

| ID / area | Current position | Remaining closure criteria |
|---|---|---|
| **G01 Documentation** | Scoped improvement: playtest handoff, diagnostic/recovery guidance and current evidence are connected. | Maintain current links, compatibility, delivery hashes and truthful claims each release; preserve historical measurements. |
| **G02 Windows delivery** | Open; **P1 for the user's Windows PC**. | Execute Windows gates and a clean-machine create/play/save/reopen/recovery journey; name supported OS/GPU/runtime and delivery policy. The Linux ZIP cannot satisfy this. |
| **G03 Career progression** | Reviewed rental returns and accepted-stop handoffs are safer; prior priced-career and continuing-supply cases remain. | Test beginner/advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and longer sessions without invented funding. |
| **G04 Continuing goals** | Six explicit goals and owner commitments retained; contextual driving handoff repaired; playtest protocol supplied. | Observe uncoached first/returning Supply, Home/Community and Road/Company journeys and measure misunderstood costs, stalls and recovery. |
| **G05 Navigation/recovery** | Existing physical, narrow-road and clustered-obstacle regression matrices remain. | Expand declared geometry, density and long interactions; require bounded progress or explicit value-preserving recovery. |
| **G06 Community lifecycle** | Existing departures, removals and handover retained. | Define unavailable-leader succession, dissolution, transfer/reclaim and treasury policy; verify offline permission and durable values. |
| **G07 Rendering qualification** | Bounded model/loading and timing evidence is easier to retain. | Measure frame tails, real residency and loading peaks on selected GPUs; production overrides require equivalent budgets. |
| **G08 Accessibility/devices** | Contextual bindings and vehicle status corrected; fresh limited native review recorded. | Qualify full physical-device journeys, reconnect ordering, assistive access, contrast, motion comfort and localization with representative users. |
| **G09 Production content** | Existing validated, original wildlife/vehicle/human examples preserved. | Extend skinning/retargeting, facial/equipment work, boarding, passengers/remote occupancy, audio, DCC presets and shipping budgets/licensing. |
| **G10 Regional data** | Connected prepared input/seam/coverage contracts retained. | Choose licensed shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry. |
| **G11 Trusted delivery** | Exact local artifacts and manifests retained as integrity evidence. | Add publisher trust/signing, release discovery, authenticated transfer/resume, installed-version selection and rollback. Hashes alone do not authenticate a publisher. |
| **G12 Content migration** | Exact recovery and source identity remain; no migration introduced. | Design explicit ID/geometry/catalog conflict migrations with backup, rollback and authoritative validation. |
| **G13 Public service controls** | Open; conditional **P1 for Internet hosting**. | Establish enrollment/recovery, authenticated operators, durable moderation/audit and abuse controls. |
| **G14 WAN and capacity** | Finite loopback/session workloads remain the acceptance boundary. | Run representative delay/loss and long soaks; derive supported populations and latency targets from measured results. |
| **G15 Storage stalls** | Scoped improvement: terminal worker loss no longer masquerades as pending, and uncertain commits remain blocked. | Measure transition/maintenance latency tails and reduce stalls without weakening complete validation or durable acknowledgements. Final disk operations can still wait. |
| **G16 Operational recovery** | Scoped improvement: actionable terminal close/reopen joins named backup/recovery and exact committed-state checks. | Set retention, confidentiality, offsite copies, owners and RPO/RTO; rehearse the intended deployment and extend unsupported source/catalog policy explicitly. |
| **G17 Observability** | Scoped improvement: menu opt-in, bounded local history, status/age, session phase and single-writer failure handling. | Add deployment retention, crash diagnostics, external audit/alerts and ownership; validate detection. Stale/absent captures never establish health. |
| **G18 Global architecture** | Conditional expansion; regional single authority remains explicit. | Establish actual population/data requirements before authority partitions, cross-shard ownership/economy and adaptive delivery. |

## Recommended next iteration

The highest-value next evidence is a clean Windows/hardware run and several
uncoached new/returning sessions using this handoff. Collect the observed blocker,
the player's interpretation, committed checkpoint and reviewed diagnostic tail.
Choose subsequent economy/content/traffic work from repeated failures rather
than raising world budgets without measurements. If public Internet hosting
becomes the release goal, prioritize G13–G17 deployment qualification before
larger-scale architecture.
